# Contributing to skunk 🦨

First off, thank you for considering contributing to skunk 🦨.

There are plenty of [issues][1] to work on. Look for the [good first issue][3]
label if you want something easier first.

If your contribution is not straightforward, please first discuss the change
you wish to make in the relevant issue, or create a new one, if there isn't one
already.


## Reporting issues

Before reporting an issue on the [issue tracker][1], please check that it has
not already been reported by searching for some related keywords.

Try to use a clear title, and describe your problem with complete sentences.


## Workspace Setup

skunk currently requires rust nightly: `rustup override set nightly`.

The project's [README][2] has some guidance on how to setup the project for
development.


## Workflow

After making your changes make sure your changes compile and all tests pass
with `cargo test --all-features --workspace`.

Format the code with `cargo fmt`.


## Pull requests

Try to open one pull request per feature, patch, etc.


[1]: https://github.com/jgraef/skunk/issues
[2]: https://github.com/jgraef/skunk/tree/main#development
[3]: https://github.com/jgraef/skunk/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22
