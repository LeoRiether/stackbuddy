use eyre::{eyre, Context, Error};
use std::process::{Command, Stdio};

pub fn current_stack() -> Vec<String> {
    StackIter::new().collect()
}

/// StackIter is an iterator that yields the current branch and then its parent, and so on, until
/// the main branch is reached.
#[derive(Debug, Default)]
struct StackIter {
    current: Option<String>,
}

impl StackIter {
    pub fn new() -> Self {
        let current = current_branch().expect("failed to get current branch");
        Self {
            current: Some(current),
        }
    }
}

impl Iterator for StackIter {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current.take()?;
        let next = parent(current.clone()).expect("failed to get parent branch");
        if next != current {
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
    let current = String::from_utf8(current).context("git rev-parse output was not valid utf-8")?;
    Ok(current)
}

/// Based on this: https://stackoverflow.com/questions/3161204/how-to-find-the-nearest-parent-of-a-git-branch#comment51253180_17843908
/// I could have done some of the processing in Rust, sure, but I don't really want to think about
/// it :)
pub fn parent(branch: String) -> Result<String, Error> {
    let mut show_branch = Command::new("git")
        .arg("show-branch")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("on git show-branch")?;

    let mut sed = Command::new("sed")
        .arg("s/].*//")
        .stdin(show_branch.stdout.take().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("on sed s/].*//")?;

    let mut grep = Command::new("grep")
        .arg(r"\*")
        .stdin(sed.stdout.take().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context(r"on grep \*")?;

    let mut grep2 = Command::new("grep")
        .args(["-v", &branch])
        .stdin(grep.stdout.take().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("on grep -v {branch}"))?;

    let mut head = Command::new("head")
        .arg("-n1")
        .stdin(grep2.stdout.take().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("on head -n1")?;

    let sed2 = Command::new("sed")
        .arg("s/^.*\\[//") // ]
        .stdin(head.stdout.take().unwrap())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .context("on sed s/^.*\\[//")? // ]
        .stdout;

    let parent = String::from_utf8(sed2).context("failed to parse parent branch")?;
    Ok(parent)
}

pub fn pr_for_branch(_branch: String) -> Result<Option<String>, Error> {
    Ok(Some("#22".to_string()))
}

pub fn note_block(branch: String) -> Result<String, Error> {
    let stack = current_stack();

    let branch_index = stack
        .iter()
        .position(|b| b == &branch)
        .ok_or(eyre!("branch '{}' is not in stack", branch))?;

    let prev_pr = stack
        .get(branch_index + 1)
        .map(|b| pr_for_branch(b.clone()))
        .transpose()?
        .flatten()
        .unwrap_or_else(|| "(none)".to_string());
    let next_pr = stack
        .get(branch_index.wrapping_sub(1))
        .map(|b| pr_for_branch(b.clone()))
        .transpose()?
        .flatten()
        .unwrap_or_else(|| "(none)".to_string());

    Ok(format!(
        r"
> [!Note]
> Previous PR: {prev_pr}
> Next PR: {next_pr}
"
    ))
}
