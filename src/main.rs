use clap::{Parser, Subcommand};
use eyre::Error;
use stackbuddy::NoteFormat;

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

    /// Prints the stack of branches that ends in the current branch
    Stack {
        /// The branch to start the stack from. If not given, the current branch is used
        branch: Option<String>,
    },

    /// Generates a [!Note] block for the PR of the given branch
    Note {
        /// The format to display the note in
        #[arg(value_enum, default_value_t = NoteFormat::default())]
        format: NoteFormat,

        branch: Option<String>,
    },

    /// Updates all PRs in a stack, starting from the given branch, with a note. For more
    /// information about notes, see stackbuddy note --help
    UpdateNotes {
        /// The format to display the notes in
        #[arg(value_enum, default_value_t = NoteFormat::default())]
        format: NoteFormat,

        branch: Option<String>,

        #[clap(short, long, default_value_t = false)]
        dry_run: bool,
    },
}

fn main() -> Result<(), Error> {
    let args = Args::parse();

    match args.command {
        Command::Parent { branch } => {
            let branch = branch.unwrap_or_else(|| stackbuddy::current_branch().unwrap());
            let parent = stackbuddy::parent(branch)?;
            println!("{}", parent.unwrap_or_else(|| "None".to_string()));
        }
        Command::Stack { branch } => {
            let branch = branch.unwrap_or_else(|| stackbuddy::current_branch().unwrap());
            for branch in stackbuddy::stack_from(branch) {
                println!("{branch}")
            }
        }
        Command::Note { format, branch } => {
            let branch = branch.unwrap_or_else(|| stackbuddy::current_branch().unwrap());
            let note = stackbuddy::note_block(branch, format)?;
            println!("{note}");
        }
        Command::UpdateNotes {
            format,
            branch,
            dry_run,
        } => {
            let branch = branch.unwrap_or_else(|| stackbuddy::current_branch().unwrap());
            for branch in stackbuddy::stack_from(branch) {
                println!("Updating notes for {branch}...");
                if let Err(e) = stackbuddy::update_note(branch.clone(), format, dry_run) {
                    println!("Error in branch {branch}: {e}")
                }
            }
        }
    }

    Ok(())
}
