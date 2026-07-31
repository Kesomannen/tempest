# Tempest

Tempest is an experimental CLI tool for managing mods for various games, primarily focused on the Thunderstore ecosystem. 

## Features

- Manage mods with a manifest-based profile format inspired by Cargo.
- Install mods from multiple sources, including Thunderstore, Github Releases and local files.
- Automatic support for most games on Thunderstore thanks to the [schema API endpoint](https://thunderstore.io/api/experimental/schema/dev/latest/).
- Share and version control profiles easily with git or standard Thunderstore profile codes.

## Installation

Prebuilt binaries are currently not available.

To build Tempest, the Rust toolchain is required. To begin, clone the repository:

```
git clone https://github.com/Kesomannen/tempest.git
```

Navigate to the project directory and run:

```
cargo build --release
```

The binary will be built to `target/release`.

## Usage

Create a new profile directory with `new`:

```bash
tempest new MyProfile lethal-company
```

> [!NOTE]
> Unlike traditional mod managers like [r2modman](https://github.com/ebkr/r2modmanPlus) and [Gale](https://github.com/Kesomannen/gale), Tempest does not store mod profiles in a central directory. Instead it works similarly to tools like `git`; any directory with a valid `tempest.toml` is recognized as a profile.

Navigate to the new directory:
```bash
cd MyProfile
```

Add a Thunderstore mod to the manifest and install it along with dependencies by using `add`:

```bash
tempest add x753-Mimcs
```

> [!TIP]
> You can omit any part of the name and Tempest will do its best to find the mod. For example, try excluding `x753-` from the command above and pick it from the search results instead.

List all installed mods with `list`:

```bash
tempest list
```

> [!NOTE]
> You will see `BepInExPack` marked as "transitive". This means it was required as a dependency and installed automatically, but not mentioned explicitly in the profile manifest.

Launch the game using the default platform with `launch`:

```bash
tempest launch
```

Either share your mods by using normal git commands, or create a Thunderstore profile code:

```bash
tempest export
```
