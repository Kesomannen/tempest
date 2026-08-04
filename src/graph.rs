use std::{collections::HashMap, fmt::Write};

use anyhow::{Result, bail};
use loadsmith::{Dependency, LockedPackage, Lockfile, PackageId};

#[derive(Debug)]
pub(crate) struct DependencyGraph<'a> {
    packages_by_id: HashMap<&'a PackageId, &'a LockedPackage>,
    dependants_by_id: HashMap<&'a PackageId, Vec<&'a LockedPackage>>,
    roots: Vec<&'a LockedPackage>,
}

impl<'a> DependencyGraph<'a> {
    pub(crate) fn new(lockfile: &'a Lockfile) -> Self {
        let packages = lockfile.packages().iter().collect::<Vec<_>>();
        let packages_by_id = packages
            .iter()
            .map(|package| (package.ref_.id(), *package))
            .collect();

        let mut dependants_by_id = HashMap::<&'a PackageId, Vec<&'a LockedPackage>>::new();
        for package in &packages {
            for dependency in &package.deps {
                dependants_by_id
                    .entry(&dependency.id)
                    .or_default()
                    .push(*package);
            }
        }

        for dependants in dependants_by_id.values_mut() {
            dependants.sort_by(|left, right| left.ref_.cmp(&right.ref_));
        }

        let mut roots = packages
            .iter()
            .copied()
            .filter(|package| !package.transitive)
            .collect::<Vec<_>>();

        if roots.is_empty() {
            roots = packages;
        }

        roots.sort_by(|left, right| left.ref_.cmp(&right.ref_));

        Self {
            packages_by_id,
            dependants_by_id,
            roots,
        }
    }

    pub(crate) fn render(&self) -> String {
        GraphRenderer::new(self, false).render(self.roots.iter().copied())
    }

    pub(crate) fn render_targeted(&self, targets: &[String], reverse: bool) -> Result<String> {
        let roots = self.resolve_targets(targets)?;

        if reverse && roots.is_empty() {
            bail!("reverse tree requires at least one package target");
        }

        Ok(GraphRenderer::new(self, reverse).render(roots))
    }

    fn resolve_targets(&self, targets: &[String]) -> Result<Vec<&'a LockedPackage>> {
        if targets.is_empty() {
            return Ok(self.roots.clone());
        }

        let mut resolved = targets
            .iter()
            .map(|target| self.resolve_target(target))
            .collect::<Result<Vec<_>>>()?;

        resolved.sort_by(|left, right| left.ref_.cmp(&right.ref_));
        resolved.dedup_by(|left, right| left.ref_ == right.ref_);

        Ok(resolved)
    }

    fn resolve_target(&self, query: &str) -> Result<&'a LockedPackage> {
        let exact = PackageId::new(query);

        if let Some(package) = self
            .packages_by_id
            .iter()
            .find_map(|(package_id, package)| (*package_id == &exact).then_some(*package))
        {
            return Ok(package);
        }

        let lower_query = query.to_lowercase();
        let mut matches = self
            .packages_by_id
            .values()
            .copied()
            .filter(|package| {
                package
                    .ref_
                    .id()
                    .as_str()
                    .to_lowercase()
                    .split('-')
                    .any(|segment| segment == lower_query)
            })
            .collect::<Vec<_>>();

        match matches.len() {
            0 => bail!("no packages found matching '{query}'"),
            1 => Ok(matches.swap_remove(0)),
            count => bail!("multiple packages found matching '{query}' ({count})"),
        }
    }

    fn children(&self, package: &'a LockedPackage, reverse: bool) -> Vec<Child<'a>> {
        let mut children = if reverse {
            self.dependants_by_id
                .get(package.ref_.id())
                .into_iter()
                .flatten()
                .copied()
                .map(Child::Resolved)
                .collect::<Vec<_>>()
        } else {
            package
                .deps
                .iter()
                .map(|dependency| {
                    self.packages_by_id
                        .get(&dependency.id)
                        .copied()
                        .map(Child::Resolved)
                        .unwrap_or_else(|| Child::Missing(dependency))
                })
                .collect::<Vec<_>>()
        };

        children.sort_by(compare_children);
        children
    }
}

struct GraphRenderer<'a> {
    graph: &'a DependencyGraph<'a>,
    reverse: bool,
    ancestor_last: Vec<bool>,
    stack: Vec<&'a PackageId>,
    out: String,
}

impl<'a> GraphRenderer<'a> {
    fn new(graph: &'a DependencyGraph<'a>, reverse: bool) -> Self {
        Self {
            graph,
            reverse,
            ancestor_last: Vec::new(),
            stack: Vec::new(),
            out: String::new(),
        }
    }

    fn render(mut self, roots: impl IntoIterator<Item = &'a LockedPackage>) -> String {
        let roots = roots.into_iter().collect::<Vec<_>>();

        for (index, root) in roots.iter().enumerate() {
            if index > 0 {
                self.out.push('\n');
            }

            self.render_node(root, None);
        }

        self.out
    }

    fn render_node(&mut self, package: &'a LockedPackage, branch: Option<bool>) {
        self.write_prefix(branch);
        let _ = writeln!(self.out, "{}", package.ref_.to_string());

        if self.stack.iter().any(|seen| *seen == package.ref_.id()) {
            return;
        }

        self.stack.push(package.ref_.id());

        if let Some(is_last) = branch {
            self.ancestor_last.push(is_last);
        }

        let children = self.graph.children(package, self.reverse);
        for (index, child) in children.iter().enumerate() {
            let is_last = index + 1 == children.len();
            match child {
                Child::Resolved(child_package) => self.render_node(child_package, Some(is_last)),
                Child::Missing(dependency) => self.render_missing(dependency, is_last),
            }
        }

        if branch.is_some() {
            self.ancestor_last.pop();
        }

        self.stack.pop();
    }

    fn render_missing(&mut self, dependency: &Dependency, is_last: bool) {
        self.write_prefix(Some(is_last));
        let _ = writeln!(
            self.out,
            "{} {} (missing)",
            dependency.id, dependency.version_req
        );
    }

    fn write_prefix(&mut self, branch: Option<bool>) {
        for is_last in &self.ancestor_last {
            self.out.push_str(if *is_last { "    " } else { "│   " });
        }

        if let Some(is_last) = branch {
            self.out.push_str(if is_last { "└── " } else { "├── " });
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Child<'a> {
    Resolved(&'a LockedPackage),
    Missing(&'a Dependency),
}

fn compare_children(left: &Child<'_>, right: &Child<'_>) -> std::cmp::Ordering {
    compare_child_key(left).cmp(&compare_child_key(right))
}

fn compare_child_key(child: &Child<'_>) -> String {
    match child {
        Child::Resolved(package) => package.ref_.to_string(),
        Child::Missing(dependency) => format!("{}@{}", dependency.id, dependency.version_req),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use loadsmith::{PackageRef, Version, VersionReq};
    use reqwest::Url;

    fn package(
        id: &str,
        version: (u64, u64, u64),
        transitive: bool,
        deps: Vec<Dependency>,
    ) -> LockedPackage {
        LockedPackage::new(
            PackageRef::new(
                id.to_string(),
                Version::new(version.0, version.1, version.2),
            ),
            "thunderstore",
            Url::parse("https://example.com/pkg.zip").unwrap(),
        )
        .with_transitive(transitive)
        .with_deps(deps)
    }

    #[test]
    fn renders_a_tree() {
        let lockfile = Lockfile::new(vec![
            package(
                "author-alpha",
                (1, 0, 0),
                false,
                vec![
                    Dependency::new("author-bravo", VersionReq::STAR, "thunderstore"),
                    Dependency::new("author-charlie", VersionReq::STAR, "thunderstore"),
                ],
            ),
            package(
                "author-bravo",
                (1, 2, 3),
                true,
                vec![Dependency::new(
                    "author-delta",
                    VersionReq::STAR,
                    "thunderstore",
                )],
            ),
            package("author-charlie", (2, 0, 0), true, vec![]),
            package("author-delta", (0, 9, 0), true, vec![]),
        ]);

        let rendered = DependencyGraph::new(&lockfile).render();

        assert_eq!(
            rendered,
            concat!(
                "author-alpha@1.0.0\n",
                "├── author-bravo@1.2.3\n",
                "│   └── author-delta@0.9.0\n",
                "└── author-charlie@2.0.0\n",
            )
        );
    }

    #[test]
    fn renders_a_target_subtree() {
        let lockfile = Lockfile::new(vec![
            package(
                "author-alpha",
                (1, 0, 0),
                false,
                vec![
                    Dependency::new("author-bravo", VersionReq::STAR, "thunderstore"),
                    Dependency::new("author-charlie", VersionReq::STAR, "thunderstore"),
                ],
            ),
            package(
                "author-bravo",
                (1, 2, 3),
                true,
                vec![Dependency::new(
                    "author-delta",
                    VersionReq::STAR,
                    "thunderstore",
                )],
            ),
            package("author-charlie", (2, 0, 0), true, vec![]),
            package("author-delta", (0, 9, 0), true, vec![]),
        ]);

        let rendered = DependencyGraph::new(&lockfile)
            .render_targeted(&["author-bravo".to_string()], false)
            .unwrap();

        assert_eq!(
            rendered,
            concat!("author-bravo@1.2.3\n", "└── author-delta@0.9.0\n")
        );
    }

    #[test]
    fn renders_a_reverse_tree() {
        let lockfile = Lockfile::new(vec![
            package(
                "author-alpha",
                (1, 0, 0),
                false,
                vec![
                    Dependency::new("author-bravo", VersionReq::STAR, "thunderstore"),
                    Dependency::new("author-charlie", VersionReq::STAR, "thunderstore"),
                ],
            ),
            package(
                "author-bravo",
                (1, 2, 3),
                true,
                vec![Dependency::new(
                    "author-delta",
                    VersionReq::STAR,
                    "thunderstore",
                )],
            ),
            package("author-charlie", (2, 0, 0), true, vec![]),
            package("author-delta", (0, 9, 0), true, vec![]),
            package(
                "author-echo",
                (1, 0, 0),
                true,
                vec![Dependency::new(
                    "author-bravo",
                    VersionReq::STAR,
                    "thunderstore",
                )],
            ),
            package(
                "author-zeta",
                (2, 0, 0),
                true,
                vec![Dependency::new(
                    "author-alpha",
                    VersionReq::STAR,
                    "thunderstore",
                )],
            ),
        ]);

        let rendered = DependencyGraph::new(&lockfile)
            .render_targeted(&["author-bravo".to_string()], true)
            .unwrap();

        assert_eq!(
            rendered,
            concat!(
                "author-bravo@1.2.3\n",
                "├── author-alpha@1.0.0\n",
                "│   └── author-zeta@2.0.0\n",
                "└── author-echo@1.0.0\n",
            )
        );
    }

    #[test]
    fn falls_back_to_all_packages_when_everything_is_transitive() {
        let lockfile = Lockfile::new(vec![package("author-alpha", (1, 0, 0), true, vec![])]);

        assert_eq!(
            DependencyGraph::new(&lockfile).render(),
            "author-alpha@1.0.0\n"
        );
    }
}
