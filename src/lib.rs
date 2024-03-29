use clap::ValueEnum;
use eyre::{eyre, Context, Error, OptionExt};
use std::{
    io::Write,
    process::{Command, Stdio},
};

pub fn current_stack() -> Vec<String> {
    StackIter::new().collect()
}

pub fn stack_from(branch: String) -> Vec<String> {
    StackIter::from(branch).collect()
}

/// StackIter is an iterator that yields the current branch and then its parent, and so on, until
/// the main branch is reached.
#[derive(Debug, Default)]
struct StackIter {
    main: String,
    current: Option<String>,
}

impl StackIter {
    pub fn new() -> Self {
        Self::from(current_branch().expect("failed to get current branch"))
    }

    pub fn from(branch: String) -> Self {
        Self {
            main: main_branch().expect("failed to get main branch"),
            current: Some(branch),
        }
    }
}

impl Iterator for StackIter {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current.take()?;
        let next = parent(current.clone()).expect("failed to get parent branch");
        self.current = next
            .filter(|next| next != &self.main)
            .filter(|next| next != &current);
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

pub fn main_branch() -> Result<String, Error> {
    let branches = Command::new("git")
        .arg("branch")
        .output()
        .context("git branch failed")?
        .stdout;
    let branches = String::from_utf8(branches)?;

    let main = branches
        .lines()
        .map(|b| b.trim_start_matches("* ").trim())
        .find(|&b| b == "main" || b == "master");

    main.map(str::to_string)
        .ok_or_eyre("Main branch not found. Is it named something other than `main` or `master`?")
}

pub fn parent(branch: String) -> Result<Option<String>, Error> {
    let log = Command::new("git")
        .args(["log", "--oneline", "--graph", "--decorate"])
        .args(["--simplify-by-decoration", "--first-parent", "-n", "32"])
        .args(["--skip", "1"])
        .arg(branch)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .context(r"git log failed")?;

    let log = String::from_utf8(log.stdout)?;

    let parent = log
        .lines() // * commit (branch) message
        .map(|line| line.trim_start_matches('*').trim()) // commit (branch) message
        .filter_map(|line| line.split_once(' ')) // (branch) message
        .filter_map(|(_commit, line)| extract_branch(line))
        .map(str::to_string)
        .next();

    Ok(parent)
}

fn extract_branch(line: &str) -> Option<&str> {
    let from = line.find('(')? + 1;
    let to = line.find(')')?;

    #[allow(clippy::filter_next)]
    line[from..to]
        .split(", ")
        .map(|branch| branch.strip_prefix("HEAD -> ").unwrap_or(branch))
        .filter(|branch| !branch.starts_with("origin/"))
        .filter(|branch| !branch.starts_with("tag: "))
        .next()
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

pub fn pr_body(branch: String) -> Result<String, Error> {
    let output = Command::new("gh")
        .args(["pr", "view", &branch, "--json", "body", "--jq", ".body"])
        .output()
        .context("gh pr view failed")?;

    if !output.status.success() {
        let stderr =
            String::from_utf8(output.stderr).context("gh pr view stderr was not valid utf-8")?;
        return Err(eyre!("gh pr view failed: {}", stderr));
    }

    let body = String::from_utf8(output.stdout).context("gh pr view stdout was not valid utf-8")?;
    Ok(body)
}

pub fn set_pr_body(branch: String, body: String) -> Result<(), Error> {
    Command::new("gh")
        .args(["pr", "edit", &branch, "--body-file", "-"])
        .stdout(Stdio::null())
        .stdin(Stdio::piped())
        .spawn()
        .context("gh pr edit failed")?
        .stdin
        .ok_or_else(|| eyre!("gh pr edit stdin was not captured"))?
        .write_all(body.as_bytes())
        .context("failed to write to gh pr edit stdin")?;
    Ok(())
}

#[derive(ValueEnum, Default, Clone, Copy)]
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
    let mut items = Vec::new();
    for b in stack.iter().rev() {
        if let Some(pr) = pr_for_branch(b.clone())? {
            items.push(format!("- #{pr}"));
            if b == branch {
                items.last_mut().unwrap().push_str(" (this)");
            }
        }
    }
    Ok(items.join("\n"))
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

pub fn update_note(branch: String, note_format: NoteFormat, dry_run: bool) -> Result<(), Error> {
    let body = pr_body(branch.clone())
        .with_context(|| format!("failed to get PR body for branch '{branch}'"))?;
    let note = note_block(branch.clone(), note_format)?;
    let new_body = replace_note(&body, &note);
    if dry_run {
        println!("New PR body:\n{}", new_body);
    } else if new_body != body {
        set_pr_body(branch, new_body)?;
    }
    Ok(())
}

fn replace_note(pr_body: &str, note: &str) -> String {
    const OPEN: &str = "<!-- stackbuddy note -->";
    const CLOSE: &str = "<!-- /stackbuddy note -->";

    let open = pr_body.find(OPEN);
    let close = pr_body.find(CLOSE);

    match (open, close) {
        (Some(open), Some(close)) if open < close => {
            let before = &pr_body[..open];
            let after = &pr_body[close + CLOSE.len()..];
            format!("{before}{OPEN}\n{note}\n{CLOSE}\n{after}")
        }
        _ => format!("{OPEN}\n{note}\n{CLOSE}\n{pr_body}"),
    }
}
