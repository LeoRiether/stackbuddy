# stackbuddy
Little CLI tool to help you manage your PR stacks

```bash
$ stackbuddy help
stackbuddy helps you manage your PR stacks

Usage: stackbuddy <COMMAND>

Commands:
  parent  Prints the parent of the given branch
  stack   Prints the stack of branches that ends in the current branch
  note    Generates a [!Note] block for the PR of the given branch
  help    Print this message or the help of the given subcommand(s)
```

## Installing

Check the [Releases](https://github.com/LeoRiether/stackbuddy/releases) page.

You must have the [GitHub CLI](https://cli.github.com/) installed to use PR-related functionality. 

## Tips & Tricks

#### Creating a new PR pointing to the correct base branch
```bash
gh pr create -B `stackbuddy parent`
```

#### Pushing all of the branches in the stack
```bash
git push --force-with-lease origin `stackbuddy stack`
```
