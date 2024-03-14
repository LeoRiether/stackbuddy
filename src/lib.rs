use clap::ValueEnum;
use eyre::{eyre, Context, Error};
use std::process::{Command, Stdio};

pub fn current_stack() -> Vec<String> {
    StackIter::new().collect()
}

pub fn stack_from(branch: String) -> Vec<String> {
    StackIter {
        current: Some(branch),
        ..StackIter::default()
    }
    .collect()
}

/// StackIter is an iterator that yields the current branch and then its parent, and so on, until
/// the main branch is reached.
#[derive(Debug, Default)]
struct StackIter {
    /// For some weird reason the [`parent`] of the base branch is the branch you're on right now
    first: String,
    current: Option<String>,
}

impl StackIter {
    pub fn new() -> Self {
        let current = current_branch().expect("failed to get current branch");
        Self {
            first: current.clone(),
            current: Some(current),
        }
    }
}

impl Iterator for StackIter {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current.take()?;
        let next = parent(current.clone()).expect("failed to get parent branch");
        if next != current && next != self.first {
            self.current = Some(next);
        }
        Some(current)
    }
}

pub fn current_branch() -> Result<String, Error> {
    let current = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .context("git rev-parse failed")?
        .stdout;
    let current = String::from_utf8(current)
        .context("git rev-parse output was not valid utf-8")?
        .trim()
        .to_string();
    Ok(current)
}

/// Based on this: https://gist.github.com/joechrysler/6073741?permalink_comment_id=3108387#gistcomment-3108387
///
/// ```
/// git log --pretty=format:'%D' HEAD^ \
/// | grep 'origin/' \
/// | head -n1 \
/// | sed 's@origin/@@' \
/// | sed 's@,.*@@'
/// ```
///
/// I could have done some of the processing in Rust, sure, but I don't really want to think about
/// it :)
pub fn parent(branch: String) -> Result<String, Error> {
    let mut git_log = Command::new("git")
        .args(["log", "--pretty=format:%D", &format!("{branch}^")])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("on git log --pretty=format:'%D' <branch>^")?;

    let mut grep = Command::new("grep")
        .arg("origin/")
        .stdin(git_log.stdout.take().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context(r"on grep 'origin/'")?;

    let mut head = Command::new("head")
        .arg("-n1")
        .stdin(grep.stdout.take().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context(r"on head -n1")?;

    let mut sed = Command::new("sed")
        .arg("s@origin/@@")
        .stdin(head.stdout.take().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context(r"on sed 's@origin/@@'")?;

    let sed = Command::new("sed")
        .arg("s@,.*@@")
        .stdin(sed.stdout.take().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .context(r"on sed 's@,.*@@'")?;

    let parent = String::from_utf8(sed.stdout)
        .context("failed to parse parent branch")?
        .trim()
        .to_string();
    Ok(parent)
}

pub fn pr_for_branch(branch: String) -> Result<Option<String>, Error> {
    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            &branch,
            "--json",
            "number",
            "--template",
            "{{.number}}",
        ])
        .output()
        .context("gh pr view failed")?;

    if !output.status.success() {
        let stderr =
            String::from_utf8(output.stderr).context("gh pr view stderr was not valid utf-8")?;
        if stderr.contains("no pull requests found") {
            return Ok(None);
        }
        return Err(eyre!("gh pr view failed: {}", stderr));
    }

    let pr = String::from_utf8(output.stdout).context("gh pr view stdout was not valid utf-8")?;
    Ok(Some(pr).filter(|pr| !pr.is_empty()))
}

#[derive(ValueEnum, Default, Clone)]
pub enum NoteFormat {
    /// Displays the previous and next PRs, like a doubly linked list
    #[default]
    Double,

    /// Displays the entire stack of PRs in a list
    List,

    /// Displays the previous and next PRs, formatted in two columns of a table
    Table,
}

pub fn note_block(branch: String, format: NoteFormat) -> Result<String, Error> {
    let stack = current_stack();

    let branch_index = stack
        .iter()
        .position(|b| b == &branch)
        .ok_or(eyre!("branch '{}' is not in the stack", branch))?;

    let prev_pr = stack
        .get(branch_index + 1)
        .filter(|_| branch_index + 2 < stack.len()) // base branch shouldn't have a PR
        .map(|b| pr_for_branch(b.clone()))
        .transpose()?
        .flatten();
    let next_pr = stack
        .get(branch_index.wrapping_sub(1))
        .map(|b| pr_for_branch(b.clone()))
        .transpose()?
        .flatten();

    match format {
        NoteFormat::Double => note_double(prev_pr, next_pr),
        NoteFormat::List => note_list(&branch, &stack),
        NoteFormat::Table => note_table(prev_pr, next_pr),
    }
}

fn note_double(prev_pr: Option<String>, next_pr: Option<String>) -> Result<String, Error> {
    let mut note = "> [!Note]".to_string();
    if let Some(prev_pr) = prev_pr {
        note.push_str(&format!("\n> - Previous PR: #{prev_pr}"));
    }
    if let Some(next_pr) = next_pr {
        note.push_str(&format!("\n> - Next PR: #{next_pr}"));
    }
    if note == "> [!Note]" {
        note.push_str("\n> This is currently the only PR in the stack");
    }
    Ok(note)
}

fn note_list(branch: &str, stack: &[String]) -> Result<String, Error> {
    let mut note = "> [!Note]\n> PRs in the stack:".to_string();
    for b in stack {
        if let Some(pr) = pr_for_branch(b.clone())? {
            note.push_str(&format!("\n> - #{pr}"));
            if b == branch {
                note.push_str(" (this)");
            }
        }
    }
    Ok(note)
}

fn note_table(prev_pr: Option<String>, next_pr: Option<String>) -> Result<String, Error> {
    let prev_pr = prev_pr
        .map(|pr| format!("#{pr}"))
        .unwrap_or_else(|| "None".to_string());
    let next_pr = next_pr
        .map(|pr| format!("#{pr}"))
        .unwrap_or_else(|| "None".to_string());

    let mut note = String::new();
    note.push_str("| Previous PR | Next PR |\n");
    note.push_str("|-------------|---------|\n");
    note.push_str(&format!("| {prev_pr} | {next_pr} |"));
    Ok(note)
}
