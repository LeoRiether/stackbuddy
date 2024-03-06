use clap::{Parser, Subcommand};
use eyre::Error;

/// stackbuddy helps you manage your PR stacks
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Prints the parent of the given branch
    Parent {
        /// The branch to find the parent of. If not given, the current branch is used
        branch: Option<String>,
    },

    /// Prints the stack that ends in the current branch
    Stack,

    /// Generates a [!Note] block for the PR of the given branch
    Note { branch: Option<String> },
}

fn main() -> Result<(), Error> {
    let args = Args::parse();

    match args.command {
        Command::Parent { branch } => {
            let branch = branch.unwrap_or_else(|| stackbuddy::current_branch().unwrap());
            let parent = stackbuddy::parent(branch).unwrap();
            println!("{parent}");
        }
        Command::Stack => {
            for branch in stackbuddy::current_stack() {
                println!("{branch}");
            }
        }
        Command::Note { branch } => {
            let branch = branch.unwrap_or_else(|| stackbuddy::current_branch().unwrap());
            let note = stackbuddy::note_block(branch)?;
            println!("{note}");
        }
    }

    Ok(())
}
