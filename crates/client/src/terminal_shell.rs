//! Fake unix shell for in-world terminals (Pass F). Pure logic, no Bevy —
//! `terminal.rs` feeds it keystrokes and renders [`Shell::screen_text`] onto
//! the screen quad. Commands: ls, cd, cat, pwd, help, clear, echo (+ a few
//! free flavor ones). The filesystem is a small static tree of lore files;
//! later rounds hang gameplay hooks (door codes, hacking) off it.

use std::collections::VecDeque;

/// Directories of the in-memory fs (absolute, no trailing slash; "" = root).
const DIRS: &[&str] = &[
    "",
    "/bin",
    "/etc",
    "/home",
    "/home/operator",
    "/var",
    "/var/log",
    "/sys",
    "/sys/doors",
    "/sys/reactor",
];

/// Files: (absolute path, contents). Kept short — the screen is ~17 rows.
const FILES: &[(&str, &str)] = &[
    ("/etc/motd", "Unauthorized access is a violation of facility code 7.\nAll sessions are logged. Have a productive shift."),
    ("/etc/passwd", "root:x:0:0:root:/root:/bin/sh\noperator:x:1000:1000::/home/operator:/bin/sh"),
    ("/home/operator/notes.txt", "day 212 -\nthe freight lift STILL rattles past level -2.\nmaintenance says it's 'within tolerance'. sure.\nkeep hearing something in the vents near the hub."),
    ("/home/operator/todo.txt", "- swap coolant filter (sector 3)\n- reset door controller AGAIN\n- ask V. about the missing pallet\n- stop reading the incident log before sleep"),
    ("/var/log/incident.log", "0412 breach alarm B-wing (false positive?)\n0413 two heat signatures in maintenance shaft\n0414 signatures lost. sensors nominal.\n0415 alarm cleared by night shift. no follow-up."),
    ("/var/log/door.log", "door-07 cycle ok\ndoor-12 cycle ok\ndoor-03 FAULT: obstruction, override used\ndoor-03 fault cleared 40s later. no operator on shift."),
    ("/sys/doors/manifest", "door-03  maintenance   OVERRIDE ENABLED\ndoor-07  hub gate      normal\ndoor-12  freight       normal"),
    ("/sys/reactor/status", "core temp: 341K (nominal)\noutput: 87%\nlast scram: 6112 days ago\ndo not believe the gauge on the floor above."),
    ("/bin/sh", "[binary]"),
];

/// Everything the terminal screen shows: scrollback + the line being edited.
pub struct Shell {
    hostname: String,
    user: &'static str,
    /// cwd as path components ("/home/operator" = ["home", "operator"]).
    cwd: Vec<String>,
    scroll: VecDeque<String>,
    pub input: String,
    cols: usize,
    rows: usize,
}

/// Per-faction hostname + login banner so a synth box doesn't greet like a
/// factory PLC. `kit` is the piece's kit path ("factions/synth", "factory"…).
fn identity(kit: &str, seed: u32) -> (String, &'static str) {
    let n = seed % 90 + 10;
    if kit.contains("synth") {
        (format!("synth-node-{n}"), "SYNTH COLLECTIVE // uplink stable")
    } else if kit.contains("factory") {
        (format!("plant-ctl-{n}"), "PLANT CONTROL bus online")
    } else {
        (format!("term-{n}"), "FACILITY OS 0.9")
    }
}

impl Shell {
    pub fn new(kit: &str, seed: u32, cols: usize, rows: usize) -> Self {
        let (hostname, banner) = identity(kit, seed);
        let mut sh = Self {
            hostname,
            user: "operator",
            cwd: Vec::new(),
            scroll: VecDeque::new(),
            input: String::new(),
            cols,
            rows,
        };
        sh.push_line(banner.to_string());
        sh.push_line(format!("{} tty1", sh.hostname));
        sh.push_line("type 'help' for commands".to_string());
        sh.push_line(String::new());
        sh
    }

    // --- input -------------------------------------------------------------

    pub fn type_char(&mut self, c: char) {
        if self.input.len() < self.cols.saturating_sub(self.prompt().len() + 1)
            && (' '..='~').contains(&c)
        {
            self.input.push(c);
        }
    }

    pub fn backspace(&mut self) {
        self.input.pop();
    }

    pub fn submit(&mut self) {
        let line = std::mem::take(&mut self.input);
        let prompt = self.prompt();
        self.push_line(format!("{prompt}{line}"));
        let mut parts = line.split_whitespace();
        let Some(cmd) = parts.next() else { return };
        let args: Vec<&str> = parts.collect();
        match cmd {
            "help" => {
                self.push_line("ls cd cat pwd echo clear help".to_string());
                self.push_line("whoami hostname uname exit(Esc)".to_string());
            }
            "ls" => self.cmd_ls(args.first().copied()),
            "cd" => self.cmd_cd(args.first().copied().unwrap_or("/")),
            "cat" => match args.first() {
                Some(p) => self.cmd_cat(p),
                None => self.push_line("cat: missing operand".to_string()),
            },
            "pwd" => {
                let p = self.cwd_str();
                self.push_line(p);
            }
            "echo" => self.push_line(args.join(" ")),
            "clear" => self.scroll.clear(),
            "whoami" => self.push_line(self.user.to_string()),
            "hostname" => {
                let h = self.hostname.clone();
                self.push_line(h);
            }
            "uname" => self.push_line("FabledOS 0.9.1 sector-kernel".to_string()),
            "exit" | "logout" => self.push_line("(press Esc to disconnect)".to_string()),
            other => self.push_line(format!("sh: {other}: command not found")),
        }
    }

    // --- commands ----------------------------------------------------------

    fn cmd_ls(&mut self, arg: Option<&str>) {
        let Some(dir) = self.resolve(arg.unwrap_or(".")) else {
            self.push_line(format!("ls: {}: no such directory", arg.unwrap_or(".")));
            return;
        };
        let prefix = join(&dir);
        if !DIRS.contains(&prefix.as_str()) {
            self.push_line(format!("ls: {}: no such directory", arg.unwrap_or(".")));
            return;
        }
        let mut entries: Vec<String> = Vec::new();
        for d in DIRS {
            if let Some(name) = child_of(&prefix, d) {
                entries.push(format!("{name}/"));
            }
        }
        for (f, _) in FILES {
            if let Some(name) = child_of(&prefix, f) {
                entries.push(name.to_string());
            }
        }
        entries.sort();
        if entries.is_empty() {
            return;
        }
        // pack a few names per row to save screen rows
        for chunk in entries.chunks(3) {
            let row = chunk
                .iter()
                .map(|e| format!("{e:<18}"))
                .collect::<String>();
            self.push_line(row.trim_end().to_string());
        }
    }

    fn cmd_cd(&mut self, arg: &str) {
        match self.resolve(arg) {
            Some(p) if DIRS.contains(&join(&p).as_str()) => self.cwd = p,
            _ => self.push_line(format!("cd: {arg}: no such directory")),
        }
    }

    fn cmd_cat(&mut self, arg: &str) {
        let Some(p) = self.resolve(arg) else {
            self.push_line(format!("cat: {arg}: no such file"));
            return;
        };
        let path = join(&p);
        match FILES.iter().find(|(f, _)| *f == path) {
            Some((_, body)) => {
                for l in body.lines() {
                    self.push_line(l.to_string());
                }
            }
            None if DIRS.contains(&path.as_str()) => {
                self.push_line(format!("cat: {arg}: is a directory"));
            }
            None => self.push_line(format!("cat: {arg}: no such file")),
        }
    }

    // --- paths ---------------------------------------------------------------

    /// Resolve `arg` against cwd into components; None on underflowing "..".
    fn resolve(&self, arg: &str) -> Option<Vec<String>> {
        let mut out: Vec<String> = if arg.starts_with('/') {
            Vec::new()
        } else {
            self.cwd.clone()
        };
        for part in arg.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    out.pop();
                }
                p => out.push(p.to_string()),
            }
        }
        Some(out)
    }

    fn cwd_str(&self) -> String {
        let s = join(&self.cwd);
        if s.is_empty() { "/".to_string() } else { s }
    }

    // --- screen --------------------------------------------------------------

    fn prompt(&self) -> String {
        let cwd = if self.cwd.is_empty() {
            "/".to_string()
        } else {
            // short prompt: last component only, like PS1='\W'
            format!("{}", self.cwd.last().unwrap())
        };
        format!("{}@{}:{}$ ", self.user, self.hostname, cwd)
    }

    fn push_line(&mut self, line: String) {
        // hard-wrap so long cat output can't run off the quad
        let mut rest = line.as_str();
        loop {
            if rest.len() <= self.cols {
                self.scroll.push_back(rest.to_string());
                break;
            }
            let (head, tail) = rest.split_at(self.cols);
            self.scroll.push_back(head.to_string());
            rest = tail;
        }
        while self.scroll.len() > 200 {
            self.scroll.pop_front();
        }
    }

    /// Full text for the screen: last rows-1 scroll lines + the edit line.
    pub fn screen_text(&self) -> String {
        let take = self.rows.saturating_sub(1);
        let start = self.scroll.len().saturating_sub(take);
        let mut out = String::new();
        for l in self.scroll.iter().skip(start) {
            out.push_str(l);
            out.push('\n');
        }
        out.push_str(&self.prompt());
        out.push_str(&self.input);
        out.push('\u{2588}'); // block cursor
        out
    }
}

fn join(parts: &[String]) -> String {
    if parts.is_empty() {
        return String::new();
    }
    format!("/{}", parts.join("/"))
}

/// If `path` is a direct child of dir `prefix`, return its name.
fn child_of<'a>(prefix: &str, path: &'a str) -> Option<&'a str> {
    let rest = path.strip_prefix(prefix)?.strip_prefix('/')?;
    (!rest.is_empty() && !rest.contains('/')).then_some(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh() -> Shell {
        Shell::new("factions/synth", 42, 58, 17)
    }

    fn run(sh: &mut Shell, cmd: &str) {
        sh.input = cmd.to_string();
        sh.submit();
    }

    fn last(sh: &Shell) -> &str {
        sh.scroll.back().map(|s| s.as_str()).unwrap_or("")
    }

    #[test]
    fn pwd_starts_at_root() {
        let mut s = sh();
        run(&mut s, "pwd");
        assert_eq!(last(&s), "/");
    }

    #[test]
    fn cd_and_pwd() {
        let mut s = sh();
        run(&mut s, "cd /home/operator");
        run(&mut s, "pwd");
        assert_eq!(last(&s), "/home/operator");
        run(&mut s, "cd ..");
        run(&mut s, "pwd");
        assert_eq!(last(&s), "/home");
        run(&mut s, "cd nope");
        assert!(last(&s).contains("no such directory"));
    }

    #[test]
    fn cd_relative_and_root() {
        let mut s = sh();
        run(&mut s, "cd var");
        run(&mut s, "cd log");
        run(&mut s, "pwd");
        assert_eq!(last(&s), "/var/log");
        run(&mut s, "cd");
        run(&mut s, "pwd");
        assert_eq!(last(&s), "/");
    }

    #[test]
    fn ls_root_lists_dirs() {
        let mut s = sh();
        let before = s.scroll.len();
        run(&mut s, "ls");
        let listed: Vec<_> = s.scroll.iter().skip(before + 1).cloned().collect();
        let all = listed.join(" ");
        assert!(all.contains("etc/"), "{all}");
        assert!(all.contains("home/"), "{all}");
        assert!(all.contains("sys/"), "{all}");
    }

    #[test]
    fn cat_file_and_errors() {
        let mut s = sh();
        run(&mut s, "cat /etc/motd");
        assert!(last(&s).contains("productive shift"));
        run(&mut s, "cat /etc");
        assert!(last(&s).contains("is a directory"));
        run(&mut s, "cat /missing");
        assert!(last(&s).contains("no such file"));
        run(&mut s, "cat");
        assert!(last(&s).contains("missing operand"));
    }

    #[test]
    fn cat_relative_after_cd() {
        let mut s = sh();
        run(&mut s, "cd /var/log");
        run(&mut s, "cat incident.log");
        assert!(s.scroll.iter().any(|l| l.contains("heat signatures")));
    }

    #[test]
    fn echo_and_unknown() {
        let mut s = sh();
        run(&mut s, "echo hello there");
        assert_eq!(last(&s), "hello there");
        run(&mut s, "frobnicate");
        assert_eq!(last(&s), "sh: frobnicate: command not found");
    }

    #[test]
    fn clear_empties_scroll() {
        let mut s = sh();
        run(&mut s, "ls");
        run(&mut s, "clear");
        assert!(s.scroll.is_empty());
        // screen still renders a prompt line
        assert!(s.screen_text().contains("operator@synth-node-"));
    }

    #[test]
    fn long_line_wraps() {
        let mut s = sh();
        let long = "x".repeat(150);
        run(&mut s, &format!("echo {long}"));
        assert!(s.scroll.iter().all(|l| l.len() <= 58));
    }

    #[test]
    fn screen_text_caps_rows_and_has_cursor() {
        let mut s = sh();
        for i in 0..40 {
            run(&mut s, &format!("echo line{i}"));
        }
        let text = s.screen_text();
        assert!(text.lines().count() <= 17);
        assert!(text.ends_with('\u{2588}'));
    }

    #[test]
    fn hostname_per_faction() {
        let s = Shell::new("factory", 7, 58, 17);
        assert!(s.hostname.starts_with("plant-ctl-"));
        let s = Shell::new("space_station", 7, 58, 17);
        assert!(s.hostname.starts_with("term-"));
    }

    #[test]
    fn typing_respects_width_and_ascii() {
        let mut s = sh();
        s.type_char('l');
        s.type_char('s');
        s.type_char('\u{1F600}');
        assert_eq!(s.input, "ls");
        s.backspace();
        assert_eq!(s.input, "l");
    }
}
