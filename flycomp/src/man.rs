use crate::{Command, Arg};
use regex::Regex;

fn remove_groff_formatting(data: &str) -> String {
    let re_xz = Regex::new(r"\\[XZ]'[^']*'").unwrap();
    let data = re_xz.replace_all(data, "");
    let re_pd = Regex::new(r"\.PD( \d+)").unwrap();
    let data = re_pd.replace_all(&data, "");

    data.replace("\\fI", "")
        .replace("\\fP", "")
        .replace("\\f1", "")
        .replace("\\fB", "")
        .replace("\\fR", "")
        .replace("\\e", "")
        .replace(".BI", "")
        .replace(".BR", "")
        .replace("0.5i", "")
        .replace(".rb", "")
        .replace("\\^", "")
        .replace("{ ", "")
        .replace(" }", "")
        .replace("\\ ", "")
        .replace("\\-", "-")
        .replace("\\&", "")
        .replace(".B", "")
        .replace(".I", "")
        .replace("\x0C", "") // \f
        .replace("\\(oq", "'")
        .replace("\\(cq", "'")
        .replace("\\(aq", "'")
        .replace("\\(dq", "\"")
        .replace("\\(lq", "\"")
        .replace("\\(rq", "\"")
}

fn unquote(s: &str) -> String {
    let mut out = s.to_string();
    if out.starts_with('"') && out.ends_with('"') && out.len() >= 2 {
        out = out[1..out.len()-1].to_string();
    }
    if out.starts_with('`') && out.ends_with('\'') && out.len() >= 2 {
        out = out[1..out.len()-1].to_string();
    }
    out
}

fn clean_description(desc: &str) -> String {
    let mut desc = desc.trim().to_string();
    desc = desc.replace("\n", " ");

    // truncate to first line/sentence roughly
    if let Some(idx) = desc.find(". ") {
        desc = desc[..idx+1].to_string();
    }
    desc
}

fn add_args(cmd: &mut Command, options: &str, description: &str) {
    let re_split = Regex::new(r"[ ,=|=]").unwrap();
    let tokens: Vec<&str> = re_split.split(options).collect();

    let mut short = None;
    let mut long = None;

    for opt in tokens {
        let opt = Regex::new(r"(\[.*\])").unwrap().replace_all(opt, "").to_string();
        let opt = opt.trim_matches(|c| " \t\r\n[](){}.,:!".contains(c));

        if opt == "-" || opt == "--" || opt.is_empty() {
            continue;
        }

        if opt.starts_with("--") {
            if long.is_none() { long = Some(opt.to_string()); }
        } else if opt.starts_with('-') && opt.len() == 2 {
            if short.is_none() { short = Some(opt.to_string()); }
        } else if opt.starts_with('-') && opt.len() > 2 {
            // Old style long option (-recursive)
            if long.is_none() { long = Some(format!("-{}", &opt[1..])); }
        }
    }

    if short.is_some() || long.is_some() {
        cmd.args.push(Arg {
            short,
            long,
            description: Some(clean_description(description)),
            value_type: None, // Could parse out
            num_args: None,
        });
    }
}

fn parse_options_type1(cmd: &mut Command, options_section: &str) -> bool {
    let re = Regex::new(r"(?ms)\.PP(.*?)\.RE").unwrap();
    let mut found = false;
    for cap in re.captures_iter(options_section) {
        let mut data = cap.get(1).unwrap().as_str().to_string();
        if let Some(idx) = data.rfind(".PP") {
            data = data[idx+3..].to_string();
        }
        data = remove_groff_formatting(&data);
        let parts: Vec<&str> = data.splitn(2, ".RS 4").collect();
        if parts.len() > 1 {
            let option_name = unquote(parts[0].trim());
            if option_name.contains('-') {
                add_args(cmd, &option_name, parts[1]);
                found = true;
            }
        }
    }
    if found { return true; }

    // Fallback 1: TP
    // Removed lookahead, replace with splitting or non-greedy matching.
    let re_tp = Regex::new(r"(?ms)\.TP(?: \d+)?\n(.*?)\n(.*?)(\.TP|\z)").unwrap();
    for cap in re_tp.captures_iter(options_section) {
        let mut option_name = remove_groff_formatting(cap.get(1).unwrap().as_str());
        let desc = remove_groff_formatting(cap.get(2).unwrap().as_str());

        option_name = unquote(option_name.trim());
        if option_name.contains('-') {
            add_args(cmd, &option_name, &desc);
            found = true;
        }
    }
    if found { return true; }

    // Fallback 2: IP
    let re_ip = Regex::new(r"(?ms)\.IP (.*?)\n(.*?)(\.IP|\z)").unwrap();
    let no_ix = Regex::new(r"(?m)^\.IX.*\n?").unwrap().replace_all(options_section, "").to_string();
    for cap in re_ip.captures_iter(&no_ix) {
        let mut option_name = remove_groff_formatting(cap.get(1).unwrap().as_str());
        let desc = remove_groff_formatting(cap.get(2).unwrap().as_str());

        option_name = Regex::new(r"\d+$").unwrap().replace(&option_name, "").to_string();
        option_name = unquote(option_name.trim());
        if option_name.contains('-') {
            add_args(cmd, &option_name, &desc);
            found = true;
        }
    }

    found
}

fn parse_options_type2(cmd: &mut Command, options_section: &str) -> bool {
    let re = Regex::new(r"(?ms)\.[IT]P(?: \d+(?:\.\d)?i?)?\n(.*?)\n(.*?)(\.[IT]P|\.UNINDENT|\.UN|\.SH|\z)").unwrap();
    let mut found = false;
    for cap in re.captures_iter(options_section) {
        let option_name = remove_groff_formatting(cap.get(1).unwrap().as_str());
        let desc = remove_groff_formatting(cap.get(2).unwrap().as_str());
        let option_name = unquote(option_name.trim());
        if option_name.contains('-') {
            add_args(cmd, &option_name, &desc);
            found = true;
        }
    }
    found
}

fn parse_options_darwin(cmd: &mut Command, options_section: &str) -> bool {
    let mut found = false;
    let mut lines = options_section.lines().peekable();

    while let Some(line) = lines.next() {
        if line.starts_with(".It Fl") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() > 2 {
                let name = parts[2];
                let mut dashes = "-".to_string();
                if line.contains("Fl Fl") {
                    dashes = "--".to_string();
                }

                let mut desc = String::new();
                while let Some(&next) = lines.peek() {
                    if next.starts_with(".It Fl") || next.starts_with(".Sh") {
                        break;
                    }
                    if !next.starts_with(".\\\"") {
                        let cl = remove_groff_formatting(next).replace(".Nm", cmd.name.as_deref().unwrap_or(""));
                        desc.push_str(&cl);
                        desc.push(' ');
                    }
                    lines.next();
                }

                add_args(cmd, &format!("{}{}", dashes, name), &desc);
                found = true;
            }
        }
    }

    found
}

// Fallback logic for ssh/sudo which might lack .SH OPTIONS but have arguments in DESCRIPTION or similar
fn parse_options_deroff(cmd: &mut Command, content: &str) -> bool {
    // A simplified deroff approach: line by line, if starts with "-" it's an option.
    let content = remove_groff_formatting(content);
    let mut lines = content.lines().peekable();
    let mut found = false;
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.starts_with('-') && !trimmed.starts_with("-- ") && !trimmed.starts_with("---") {
            let option_name = trimmed.split_whitespace().next().unwrap_or("");
            if option_name.contains('-') {
                let mut desc = String::new();
                while let Some(&next) = lines.peek() {
                    let next_trimmed = next.trim();
                    if next_trimmed.starts_with('-') || next_trimmed.starts_with(".SH") {
                        break;
                    }
                    if !next_trimmed.is_empty() {
                        desc.push_str(next_trimmed);
                        desc.push(' ');
                    }
                    lines.next();
                }
                add_args(cmd, option_name, &desc);
                found = true;
            }
        }
    }
    found
}

pub fn parse_manpage(cmd_name: &str, content: &str) -> Option<Command> {
    let mut cmd = Command {
        name: Some(cmd_name.to_string()),
        description: None,
        args: vec![],
        subcommands: vec![],
        author: None,
    };

    let re_options_type1 = Regex::new(r#"(?ms)\.SH "OPTIONS"(.*?)(?:\.SH|\z)"#).unwrap();
    let re_options_type2 = Regex::new(r"(?ms)\.SH OPTIONS(.*?)(?:\.SH|\z)").unwrap();
    let re_options_type3 = Regex::new(r"(?ms)\.SH DESCRIPTION(.*?)(?:\.SH|\z)").unwrap();
    let re_options_scdoc = Regex::new(r"(?ms)\.SH OPTIONS(.*?)\.SH").unwrap();
    let re_options_darwin = Regex::new(r"(?ms)\.S[hH] DESCRIPTION(.*?)(?:\.S[hH]|\z)").unwrap();
    let re_options_darwin2 = Regex::new(r"(?ms)\.S[hH] OPTIONS(.*?)(?:\.S[hH]|\z)").unwrap();

    let mut found = false;

    if let Some(caps) = re_options_type1.captures(content) {
        found = parse_options_type1(&mut cmd, caps.get(1).unwrap().as_str());
    }
    if !found {
        if let Some(caps) = re_options_type2.captures(content) {
            found = parse_options_type2(&mut cmd, caps.get(1).unwrap().as_str());
        }
    }
    if !found {
        if let Some(caps) = re_options_scdoc.captures(content) {
             found = parse_options_type2(&mut cmd, caps.get(1).unwrap().as_str());
        }
    }
    if !found {
        if let Some(caps) = re_options_type3.captures(content) {
            found = parse_options_type1(&mut cmd, caps.get(1).unwrap().as_str());
        }
    }
    if !found {
        if let Some(caps) = re_options_darwin.captures(content) {
             found = parse_options_darwin(&mut cmd, caps.get(1).unwrap().as_str());
        }
    }
    if !found {
        if let Some(caps) = re_options_darwin2.captures(content) {
             found = parse_options_darwin(&mut cmd, caps.get(1).unwrap().as_str());
        }
    }

    if !found {
        found = parse_options_type2(&mut cmd, content);
    }
    if !found {
        found = parse_options_type1(&mut cmd, content);
    }

    // Check if it's the `find` man page which uses `.IP` but lacks `.SH OPTIONS` in some versions
    // or its options section has `.SS POSITIONAL OPTIONS` or something.
    if !found || cmd.args.is_empty() {
        found = parse_options_deroff(&mut cmd, content);
    }

    if found && !cmd.args.is_empty() {
        Some(cmd)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn parse_test_manpage(name: &str) -> Option<Command> {
        let content = fs::read_to_string(format!("../tests/man_pages/{}", name)).unwrap();
        // Since many manpages are actually named e.g. ls.1, we strip the extension for the test command name
        let cmd_name = name.split('.').next().unwrap();
        parse_manpage(cmd_name, &content)
    }

    #[test]
    fn test_parse_ls_manpage() {
        let cmd = parse_test_manpage("ls.1").expect("failed to parse ls manpage");
        assert_eq!(cmd.name.as_deref(), Some("ls"));

        let arg_a = cmd.args.iter().find(|a| a.short.as_deref() == Some("-a") || a.long.as_deref() == Some("--all"));
        assert!(arg_a.is_some(), "missing -a/--all arg");
    }

    #[test]
    fn test_parse_cat_manpage() {
        let cmd = parse_test_manpage("cat.1").expect("failed to parse cat manpage");
        assert_eq!(cmd.name.as_deref(), Some("cat"));

        let arg_n = cmd.args.iter().find(|a| a.short.as_deref() == Some("-n") || a.long.as_deref() == Some("--number"));
        assert!(arg_n.is_some(), "missing -n/--number arg");
    }

    #[test]
    fn test_parse_grep_manpage() {
        let cmd = parse_test_manpage("grep.1").expect("failed to parse grep manpage");
        assert_eq!(cmd.name.as_deref(), Some("grep"));

        let arg_i = cmd.args.iter().find(|a| a.short.as_deref() == Some("-i") || a.long.as_deref() == Some("--ignore-case"));
        assert!(arg_i.is_some(), "missing -i/--ignore-case arg");
    }

    #[test]
    fn test_parse_tar_manpage() {
        let cmd = parse_test_manpage("tar.1").expect("failed to parse tar manpage");
        assert_eq!(cmd.name.as_deref(), Some("tar"));
    }

    #[test]
    fn test_parse_cp_manpage() {
        let cmd = parse_test_manpage("cp.1").expect("failed to parse cp manpage");
        assert_eq!(cmd.name.as_deref(), Some("cp"));
    }

    #[test]
    fn test_parse_mv_manpage() {
        let cmd = parse_test_manpage("mv.1").expect("failed to parse mv manpage");
        assert_eq!(cmd.name.as_deref(), Some("mv"));
    }

    #[test]
    fn test_parse_rm_manpage() {
        let cmd = parse_test_manpage("rm.1").expect("failed to parse rm manpage");
        assert_eq!(cmd.name.as_deref(), Some("rm"));
    }

    #[test]
    fn test_parse_mkdir_manpage() {
        let cmd = parse_test_manpage("mkdir.1").expect("failed to parse mkdir manpage");
        assert_eq!(cmd.name.as_deref(), Some("mkdir"));
    }

    #[test]
    fn test_parse_chmod_manpage() {
        let cmd = parse_test_manpage("chmod.1").expect("failed to parse chmod manpage");
        assert_eq!(cmd.name.as_deref(), Some("chmod"));
    }

    #[test]
    fn test_parse_chown_manpage() {
        let cmd = parse_test_manpage("chown.1").expect("failed to parse chown manpage");
        assert_eq!(cmd.name.as_deref(), Some("chown"));
    }

    #[test]
    fn test_parse_find_manpage() {
        let cmd = parse_test_manpage("find.1").expect("failed to parse find manpage");
        assert_eq!(cmd.name.as_deref(), Some("find"));
    }

    #[test]
    fn test_parse_sudo_manpage() {
        let cmd = parse_test_manpage("sudo.8").expect("failed to parse sudo manpage");
        assert_eq!(cmd.name.as_deref(), Some("sudo"));
    }

    #[test]
    fn test_parse_ping_manpage() {
        let cmd = parse_test_manpage("ping.8").expect("failed to parse ping manpage");
        assert_eq!(cmd.name.as_deref(), Some("ping"));
    }

    #[test]
    fn test_parse_curl_manpage() {
        let cmd = parse_test_manpage("curl.1").expect("failed to parse curl manpage");
        assert_eq!(cmd.name.as_deref(), Some("curl"));
    }

    #[test]
    fn test_parse_wget_manpage() {
        let cmd = parse_test_manpage("wget.1").expect("failed to parse wget manpage");
        assert_eq!(cmd.name.as_deref(), Some("wget"));
    }

    #[test]
    fn test_parse_ssh_manpage() {
        let cmd = parse_test_manpage("ssh.1").expect("failed to parse ssh manpage");
        assert_eq!(cmd.name.as_deref(), Some("ssh"));
    }

    #[test]
    fn test_parse_git_manpage() {
        let cmd = parse_test_manpage("git.1").expect("failed to parse git manpage");
        assert_eq!(cmd.name.as_deref(), Some("git"));
    }

    #[test]
    fn test_parse_bash_manpage() {
        let cmd = parse_test_manpage("bash.1").expect("failed to parse bash manpage");
        assert_eq!(cmd.name.as_deref(), Some("bash"));
    }

    #[test]
    fn test_parse_sed_manpage() {
        let cmd = parse_test_manpage("sed.1").expect("failed to parse sed manpage");
        assert_eq!(cmd.name.as_deref(), Some("sed"));
    }

    #[test]
    fn test_parse_gawk_manpage() {
        let cmd = parse_test_manpage("gawk.1").expect("failed to parse gawk manpage");
        assert_eq!(cmd.name.as_deref(), Some("gawk"));
    }
}
