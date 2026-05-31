use crate::{Arg, Command};
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedOption {
    short: Option<String>,
    long: Option<String>,
    value_type: Option<String>,
    num_args: Option<String>,
}

fn remove_device_controls(data: &str) -> String {
    Regex::new(r"\\[XZ]'[^']*'")
        .unwrap()
        .replace_all(data, "")
        .into_owned()
}

fn replace_special_escapes(data: &str) -> String {
    data.replace(r"\(oq", "'")
        .replace(r"\(cq", "'")
        .replace(r"\(aq", "'")
        .replace(r"\(dq", "\"")
        .replace(r"\(lq", "\"")
        .replace(r"\(rq", "\"")
        .replace(r"\(em", "--")
        .replace(r"\(en", "-")
        .replace(r"\(mi", "-")
        .replace(r"\(hy", "-")
        .replace(r"\e", "\\")
        .replace(r"\-", "-")
        .replace(r"\&", "")
        .replace(r"\,", "")
        .replace(r"\/", "")
        .replace(r"\^", "")
        .replace(r"\c", "")
        .replace(r"\ ", " ")
        .replace(r"\~", " ")
        .replace(r"\:", "")
        .replace(r"\|", "")
        .replace(r"\%", "")
}

fn strip_font_escapes(data: &str) -> String {
    Regex::new(r"\\f(\([^)]{2}|\[[^\]]*\]|.)")
        .unwrap()
        .replace_all(data, "")
        .into_owned()
}

fn strip_line_comment(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if trimmed.starts_with(".\\\"") || trimmed.starts_with(".\"") {
        None
    } else {
        Some(line.to_string())
    }
}

fn trim_known_inline_macros(line: &str) -> String {
    let trimmed = line.trim();
    let mut line = trimmed.to_string();

    if let Some(stripped) = trimmed.strip_prefix('.') {
        line = stripped.to_string();

        let macro_re = Regex::new(
            r"^(?:[A-Z][A-Za-z]?|rb|Nm|Fl|Ar|Pa|Ev|Dv|Cm|Ic|No|Sq|Dq|Pq|Em|Sy|Li|Tn|Ns|Op|Oo|Oc|Xo|Xc|Xr)\s+",
        )
        .unwrap();
        while macro_re.is_match(&line) {
            line = macro_re.replace(&line, "").into_owned();
        }
    }

    if line.ends_with(" ,") || line.ends_with(" .") {
        let punctuation = line.chars().last().unwrap();
        line.truncate(line.len() - 2);
        line.push(punctuation);
    }

    line
}

fn normalize_whitespace(data: &str) -> String {
    Regex::new(r"\s+")
        .unwrap()
        .replace_all(data.trim(), " ")
        .into_owned()
}

fn clean_sentence(desc: &str) -> String {
    let desc = normalize_whitespace(desc);
    if desc.is_empty() {
        return desc;
    }

    let max_len = 160;
    let mut sentences = desc
        .split('.')
        .map(str::trim)
        .filter(|segment| !segment.is_empty());
    let mut out = String::new();

    for sentence in sentences.by_ref() {
        let candidate = if out.is_empty() {
            format!("{sentence}.")
        } else {
            format!("{out} {sentence}.")
        };
        if candidate.len() > max_len && !out.is_empty() {
            break;
        }
        out = candidate;
        if out.len() >= max_len {
            break;
        }
    }

    if out.is_empty() {
        desc.chars().take(max_len).collect::<String>()
    } else {
        out.trim_end_matches('.').to_string()
    }
}

fn strip_groff_wrappers(data: &str) -> String {
    let data = remove_device_controls(data);
    let data = strip_font_escapes(&data);
    let data = replace_special_escapes(&data);
    let data = data.replace("\x0C", " ");
    let data = Regex::new(r"(?m)^\.PD(?: \d+)?$")
        .unwrap()
        .replace_all(&data, "")
        .into_owned();
    Regex::new(r"\.([A-Z][A-Za-z]?|rb)\b")
        .unwrap()
        .replace_all(&data, "")
        .into_owned()
}

fn normalize_text(data: &str, cmd_name: &str) -> String {
    let mut lines = Vec::new();

    for raw_line in data.lines() {
        let Some(raw_line) = strip_line_comment(raw_line) else {
            continue;
        };
        let line = raw_line.replace(".Nm", cmd_name);
        let line = strip_groff_wrappers(&line);
        let line = trim_known_inline_macros(&line);
        let line = line
            .replace("\\-\\^-", "--")
            .replace("\\^-", "-")
            .replace("\\^", "")
            .replace(" Ns ", "")
            .replace(" Xo", "")
            .replace(" Xc", "")
            .replace(" Oo ", "[")
            .replace(" Oc", "]")
            .replace(" Op ", "[")
            .replace(" Ar ", " ")
            .replace(" Pa ", " ")
            .replace(" Ev ", " ")
            .replace(" Dv ", " ")
            .replace(" Cm ", " ")
            .replace(" Ic ", " ")
            .replace(" Fl Fl ", " --")
            .replace(" Fl ", " -")
            .replace("No ", "")
            .replace("Sq ", "")
            .replace("Dq ", "")
            .replace("Pq ", "")
            .replace("Em ", "")
            .replace("Sy ", "")
            .replace("Li ", "")
            .replace("Tn ", "")
            .replace("Ux", "Unix")
            .replace("Bx", "BSD");
        let line = normalize_whitespace(&line);
        if !line.is_empty() {
            lines.push(line);
        }
    }

    lines.join("\n")
}

fn unquote(data: &str) -> String {
    let trimmed = data.trim();
    if trimmed.len() >= 2 {
        if (trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('`') && trimmed.ends_with('\''))
        {
            return trimmed[1..trimmed.len() - 1].trim().to_string();
        }
    }
    trimmed.to_string()
}

fn clean_option_source(data: &str, cmd_name: &str) -> String {
    normalize_text(data, cmd_name)
        .replace('\n', ", ")
        .replace('"', "")
        .replace(" [ ", "[")
        .replace(" ]", "]")
        .replace(" ,", ",")
        .replace(" :", ":")
        .replace(" =", "=")
        .replace("= ", "=")
        .replace(" / ", "/")
}

fn split_aliases(option_text: &str) -> Vec<String> {
    Regex::new(r"\s*(?:,|\||/|\bor\b)\s*")
        .unwrap()
        .split(option_text)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn normalize_value_token(token: &str) -> Option<String> {
    let token = token.trim_matches(|ch: char| ch.is_whitespace() || matches!(ch, ',' | ';' | '.'));
    if token.is_empty() {
        return None;
    }
    let token = token.trim_matches(|ch| matches!(ch, '[' | ']'));
    let token = token.trim();
    if token.is_empty() || token.starts_with('-') {
        return None;
    }
    Some(token.to_string())
}

fn find_value_type(remainder: &str) -> (Option<String>, Option<String>) {
    let remainder = remainder.trim();
    if remainder.is_empty() {
        return (None, None);
    }

    if let Some(caps) = Regex::new(r"^\[=?(?P<value>[^\]]+)\]")
        .unwrap()
        .captures(remainder)
    {
        let value = normalize_value_token(caps.name("value").unwrap().as_str());
        return (value, Some("?".to_string()));
    }

    if let Some(value) = remainder.strip_prefix('=') {
        let value = value
            .split_whitespace()
            .next()
            .and_then(normalize_value_token);
        return (value, Some("1".to_string()));
    }

    let candidate = remainder
        .split_whitespace()
        .next()
        .and_then(normalize_value_token);
    if let Some(value) = candidate {
        if value.chars().all(|ch| ch.is_ascii_digit()) {
            return (None, None);
        }
        return (Some(value), Some("1".to_string()));
    }

    (None, None)
}

fn parse_alias(alias: &str) -> Option<ParsedOption> {
    let alias = alias.trim();
    let caps = Regex::new(r"^(?P<option>--?[A-Za-z0-9#][A-Za-z0-9_-]*)(?P<rest>.*)$")
        .unwrap()
        .captures(alias)?;
    let option = caps.name("option").unwrap().as_str();
    if option == "-" || option == "--" {
        return None;
    }

    let rest = caps.name("rest").map(|m| m.as_str()).unwrap_or("");
    let (value_type, num_args) = find_value_type(rest);
    let mut parsed = ParsedOption {
        short: None,
        long: None,
        value_type,
        num_args,
    };

    if option.starts_with("--") {
        parsed.long = Some(option.to_string());
    } else if option.len() == 2 {
        parsed.short = Some(option.to_string());
    } else {
        parsed.long = Some(format!("--{}", &option[1..]));
    }

    Some(parsed)
}

fn parse_option_declaration(option_text: &str, cmd_name: &str) -> Vec<ParsedOption> {
    let option_text = clean_option_source(option_text, cmd_name);
    let option_text = unquote(&option_text);
    let aliases = split_aliases(&option_text);
    let mut parsed = Vec::new();
    let mut pending_short: Option<ParsedOption> = None;

    for alias in aliases {
        let Some(current) = parse_alias(&alias) else {
            continue;
        };

        match (&pending_short, &current.short, &current.long) {
            (Some(existing), None, Some(_)) if existing.long.is_none() => {
                let mut merged = existing.clone();
                merged.long = current.long.clone();
                if current.value_type.is_some() {
                    merged.value_type = current.value_type.clone();
                    merged.num_args = current.num_args.clone();
                } else if merged.value_type.is_none() {
                    merged.value_type = current.value_type.clone();
                    merged.num_args = current.num_args.clone();
                }
                parsed.push(merged);
                pending_short = None;
            }
            (Some(existing), Some(_), None) => {
                parsed.push(existing.clone());
                pending_short = Some(current);
            }
            _ if current.short.is_some() && current.long.is_none() => {
                if let Some(existing) = pending_short.replace(current) {
                    parsed.push(existing);
                }
            }
            _ => {
                parsed.push(current);
            }
        }
    }

    if let Some(existing) = pending_short {
        parsed.push(existing);
    }

    parsed
}

fn looks_like_option_block(option_text: &str, cmd_name: &str) -> bool {
    let Some(first_line) = option_text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with(".PD"))
    else {
        return false;
    };

    !parse_option_declaration(first_line, cmd_name).is_empty()
}

fn merge_arg(existing: &mut Arg, incoming: ParsedOption, description: &str) {
    if existing.short.is_none() {
        existing.short = incoming.short;
    }
    if existing.long.is_none() {
        existing.long = incoming.long;
    }
    if existing.value_type.is_none() {
        existing.value_type = incoming.value_type;
    }
    if existing.num_args.is_none() {
        existing.num_args = incoming.num_args;
    }
    if existing
        .description
        .as_deref()
        .unwrap_or_default()
        .is_empty()
        && !description.is_empty()
    {
        existing.description = Some(description.to_string());
    }
}

fn add_option(cmd: &mut Command, option_text: &str, description: &str) -> bool {
    let description = clean_sentence(&normalize_text(
        description,
        cmd.name.as_deref().unwrap_or(""),
    ));
    let parsed_options = parse_option_declaration(option_text, cmd.name.as_deref().unwrap_or(""));
    let mut added = false;

    for parsed in parsed_options {
        let key_short = parsed.short.clone();
        let key_long = parsed.long.clone();
        if key_short.is_none() && key_long.is_none() {
            continue;
        }

        if let Some(existing) = cmd.args.iter_mut().find(|arg| {
            (key_short.is_some() && arg.short == key_short)
                || (key_long.is_some() && arg.long == key_long)
        }) {
            merge_arg(existing, parsed, &description);
        } else {
            cmd.args.push(Arg {
                short: parsed.short,
                long: parsed.long,
                description: if description.is_empty() {
                    None
                } else {
                    Some(description.clone())
                },
                value_type: parsed.value_type,
                num_args: parsed.num_args,
            });
            added = true;
        }
    }

    added
}

fn section_title(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let title = trimmed
        .strip_prefix(".SH ")
        .or_else(|| trimmed.strip_prefix(".Sh "))?
        .trim()
        .trim_matches('"');
    Some(title.to_string())
}

fn extract_section<'a>(content: &'a str, names: &[&str]) -> Option<&'a str> {
    let mut start = None;
    let mut offset = 0;

    for line in content.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();

        if let Some(title) = section_title(line) {
            if start.is_none() && names.iter().any(|name| *name == title) {
                start = Some(offset);
                continue;
            }

            if let Some(section_start) = start {
                return Some(&content[section_start..line_start]);
            }
        }
    }

    start.map(|section_start| &content[section_start..])
}

fn top_level_sections(content: &str) -> Vec<&str> {
    let mut sections = Vec::new();
    let mut current_start = None;
    let mut offset = 0;

    for line in content.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();

        if section_title(line).is_some() {
            if let Some(section_start) = current_start.replace(line_start) {
                sections.push(&content[section_start..line_start]);
            }
        }
    }

    if let Some(section_start) = current_start {
        sections.push(&content[section_start..]);
    }

    sections
}

fn parse_type1_blocks(cmd: &mut Command, section: &str) -> bool {
    let mut found = false;
    let re = Regex::new(r"(?ms)\.PP(.*?)\.RE").unwrap();
    let cmd_name = cmd.name.clone().unwrap_or_default();

    for caps in re.captures_iter(section) {
        let mut data = caps.get(1).unwrap().as_str().to_string();
        if let Some(idx) = data.rfind(".PP") {
            data = data[idx + 3..].to_string();
        }
        let parts: Vec<&str> = data.splitn(2, ".RS 4").collect();
        if parts.len() == 2 && looks_like_option_block(parts[0], &cmd_name) {
            found |= add_option(cmd, parts[0], parts[1]);
        }
    }

    found
}

fn split_option_and_desc(line: &str) -> (String, Option<String>) {
    let trimmed = line.trim();
    if !trimmed.starts_with("\\f") {
        return (line.to_string(), None);
    }

    let mut in_format = false;
    let mut chars = trimmed.char_indices().peekable();
    let mut split_idx = None;
    let mut saw_format = false;

    while let Some(&(i, c)) = chars.peek() {
        if trimmed[i..].starts_with("\\f") {
            saw_format = true;
            let rest = &trimmed[i + 2..];
            if rest.starts_with('R')
                || rest.starts_with('P')
                || rest.starts_with("(R")
                || rest.starts_with("[R]")
                || rest.starts_with("(P")
                || rest.starts_with("[P]")
            {
                in_format = false;
            } else {
                in_format = true;
            }
            chars.next();
            chars.next();
            if let Some(&(_, next_c)) = chars.peek() {
                if next_c == '(' {
                    chars.next();
                    chars.next();
                    chars.next();
                } else if next_c == '[' {
                    while let Some(&(_, inside_c)) = chars.peek() {
                        chars.next();
                        if inside_c == ']' {
                            break;
                        }
                    }
                } else {
                    chars.next();
                }
            }
            continue;
        }

        if !in_format && saw_format && c.is_alphanumeric() {
            split_idx = Some(i);
            break;
        }

        chars.next();
    }

    if let Some(idx) = split_idx {
        let opt = trimmed[..idx].trim().to_string();
        let desc = trimmed[idx..].trim().to_string();
        let opt_cleaned = if opt.ends_with(':') {
            opt[..opt.len() - 1].trim().to_string()
        } else {
            opt
        };
        (opt_cleaned, Some(desc))
    } else {
        (line.to_string(), None)
    }
}

fn parse_tagged_blocks(cmd: &mut Command, section: &str) -> bool {
    let mut found = false;
    let no_ix = Regex::new(r"(?m)^\.IX.*\n?")
        .unwrap()
        .replace_all(section, "")
        .into_owned();

    let trailing_digits = Regex::new(r"\d+$").unwrap();
    let structural_macro =
        Regex::new(r"^\.(?:TP|TQ|IP|SH|Sh|SS|Ss|UNINDENT|UN|PP|Pp|RS|RE|sp)\b").unwrap();
    let conditional_structural_macro =
        Regex::new(r"^\.(?:ie|el)\b.*\.(?:TP|TQ|IP|HP|SH|Sh|SS|Ss|UNINDENT|UN|PP|Pp|RS|RE|sp)\b")
            .unwrap();
    let pd_macro = Regex::new(r"^\.PD(?:\s+\d+)?$").unwrap();
    let mut lines = no_ix.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        let is_tp = trimmed.starts_with(".TP") || trimmed.starts_with(".TQ");
        let is_hp = trimmed.starts_with(".HP");
        let is_ip = trimmed.starts_with(".IP ");
        if !is_tp && !is_ip && !is_hp {
            continue;
        }

        let mut opt_desc_first_line: Option<String> = None;
        let mut option_from_next_line = false;

        let mut option_name = if is_ip {
            let ip_val = trailing_digits
                .replace(trimmed.trim_start_matches(".IP").trim(), "")
                .into_owned();
            let cleaned_ip = clean_option_source(&ip_val, cmd.name.as_deref().unwrap_or(""));
            if cleaned_ip.starts_with('-') {
                ip_val
            } else {
                option_from_next_line = true;
                let mut option_line = String::new();
                while let Some(next) = lines.peek() {
                    let next_trimmed = next.trim();
                    if next_trimmed.is_empty() {
                        lines.next();
                        continue;
                    }
                    option_line = (*next).to_string();
                    lines.next();
                    break;
                }
                while let Some(next) = lines.peek() {
                    let next_trimmed = next.trim();
                    if next_trimmed.is_empty()
                        || !next_trimmed.starts_with('.')
                        || structural_macro.is_match(next_trimmed)
                    {
                        break;
                    }
                    option_line.push(' ');
                    option_line.push_str(next_trimmed);
                    lines.next();
                }
                let (opt, desc) = split_option_and_desc(&option_line);
                if let Some(d) = desc {
                    opt_desc_first_line = Some(d);
                }
                opt
            }
        } else {
            let mut option_line = String::new();
            while let Some(next) = lines.peek() {
                let next_trimmed = next.trim();
                if next_trimmed.is_empty() {
                    lines.next();
                    continue;
                }
                option_line = (*next).to_string();
                lines.next();
                break;
            }
            while let Some(next) = lines.peek() {
                let next_trimmed = next.trim();
                if next_trimmed.is_empty()
                    || !next_trimmed.starts_with('.')
                    || structural_macro.is_match(next_trimmed)
                {
                    break;
                }
                option_line.push(' ');
                option_line.push_str(next_trimmed);
                lines.next();
            }
            option_line
        };

        if is_ip && !option_from_next_line {
            while let Some(next) = lines.peek() {
                let next_trimmed = next.trim();
                if next_trimmed.is_empty() || pd_macro.is_match(next_trimmed) {
                    lines.next();
                    continue;
                }
                if next_trimmed.starts_with(".IP ") {
                    option_name.push_str(", ");
                    option_name.push_str(
                        &trailing_digits
                            .replace(next_trimmed.trim_start_matches(".IP").trim(), "")
                            .into_owned(),
                    );
                    lines.next();
                    continue;
                }
                break;
            }
        }

        let mut desc_lines = Vec::new();
        if let Some(first) = opt_desc_first_line {
            desc_lines.push(first);
        }
        while let Some(next) = lines.peek() {
            let next_trimmed = next.trim();
            if next_trimmed.is_empty() || pd_macro.is_match(next_trimmed) {
                lines.next();
                continue;
            }
            if is_hp && (next_trimmed == ".IP" || next_trimmed.starts_with(".IP ")) {
                lines.next();
                continue;
            }
            if conditional_structural_macro.is_match(next_trimmed) {
                break;
            }
            if next_trimmed.starts_with(".TP")
                || next_trimmed.starts_with(".TQ")
                || next_trimmed.starts_with(".IP ")
                || next_trimmed.starts_with(".HP")
                || next_trimmed.starts_with(".SH")
                || next_trimmed.starts_with(".Sh")
                || next_trimmed.starts_with(".SS")
                || next_trimmed.starts_with(".Ss")
                || next_trimmed.starts_with(".UNINDENT")
                || next_trimmed == ".UN"
            {
                break;
            }
            desc_lines.push((*next).to_string());
            lines.next();
        }

        found |= add_option(cmd, &option_name, &desc_lines.join("\n"));
    }

    found
}

fn parse_scdoc_blocks(cmd: &mut Command, section: &str) -> bool {
    let mut found = false;
    let re = Regex::new(r"(?ms)(.*?)\.RE").unwrap();
    let mut cursor = section;

    while let Some(caps) = re.captures(cursor) {
        let block = caps.get(1).unwrap().as_str();
        let cleaned: Vec<String> = block
            .lines()
            .map(|line| normalize_whitespace(&strip_groff_wrappers(line)))
            .filter(|line| !line.is_empty() && line != ".P" && line != ".RS 4")
            .collect();

        if cleaned.len() >= 2 {
            found |= add_option(cmd, &cleaned[0], &cleaned[1]);
        }

        cursor = &cursor[caps.get(0).unwrap().end()..];
    }

    found
}

fn parse_darwin_option_line(line: &str) -> Option<String> {
    if !line.starts_with(".It Fl") {
        return None;
    }

    let mut text = line.trim().trim_start_matches(".It").trim().to_string();
    text = format!(" {text} ")
        .replace(" Ns = Ns ", "=")
        .replace(" Ns ", "")
        .replace(" Oo ", "[")
        .replace(" Oc ", "] ")
        .replace(" Op ", "[")
        .replace(" Fl Fl ", " --")
        .replace(" Fl ", " -");
    text = Regex::new(r"(?P<prefix>^|[\s=\[:])Ar\s+")
        .unwrap()
        .replace_all(&text, "${prefix}")
        .into_owned();
    let declaration = normalize_whitespace(&strip_groff_wrappers(&text));
    if !declaration.contains('-') {
        return None;
    }

    Some(declaration)
}

fn parse_darwin(cmd: &mut Command, section: &str) -> bool {
    let mut found = false;
    let mut lines = section.lines().peekable();

    while let Some(line) = lines.next() {
        let Some(option_name) = parse_darwin_option_line(line) else {
            continue;
        };

        let mut desc_lines = Vec::new();
        while let Some(next) = lines.peek() {
            if next.starts_with(".It Fl") || next.starts_with(".Sh") || next.starts_with(".SH") {
                break;
            }
            if let Some(next_line) = strip_line_comment(next) {
                let text = normalize_text(&next_line, cmd.name.as_deref().unwrap_or(""));
                if !text.is_empty() {
                    desc_lines.push(text);
                }
            }
            lines.next();
        }

        found |= add_option(cmd, &option_name, &desc_lines.join(" "));
    }

    found
}

fn deroff(content: &str, cmd_name: &str) -> String {
    let mut out = Vec::new();

    for raw_line in content.lines() {
        let Some(raw_line) = strip_line_comment(raw_line) else {
            continue;
        };
        let trimmed = raw_line.trim_start();

        if trimmed.starts_with(".Sh ") || trimmed.starts_with(".SH ") {
            out.push(trimmed[4..].trim().trim_matches('"').to_uppercase());
            continue;
        }
        if trimmed.starts_with(".Ss ") || trimmed.starts_with(".SS ") {
            out.push(trimmed[4..].trim().trim_matches('"').to_uppercase());
            continue;
        }
        if let Some(option_name) = parse_darwin_option_line(trimmed) {
            out.push(option_name);
            continue;
        }
        if trimmed.starts_with(".PP")
            || trimmed.starts_with(".Pp")
            || trimmed.starts_with(".IP")
            || trimmed.starts_with(".TP")
            || trimmed.starts_with(".TQ")
            || trimmed.starts_with(".RS")
            || trimmed.starts_with(".RE")
            || trimmed.starts_with(".Bl")
            || trimmed.starts_with(".El")
        {
            out.push(String::new());
            continue;
        }

        let line = normalize_text(&raw_line, cmd_name);
        if !line.is_empty() {
            out.push(line);
        }
    }

    out.join("\n")
}

fn parse_deroff(cmd: &mut Command, content: &str) -> bool {
    let text = deroff(content, cmd.name.as_deref().unwrap_or(""));
    let mut lines: Vec<&str> = text.lines().collect();

    while let Some(line) = lines.first() {
        let upper = line.trim().to_uppercase();
        if upper == "DESCRIPTION" || upper == "OPTIONS" || upper == "COMMAND OPTIONS" {
            break;
        }
        lines.remove(0);
    }

    let mut found = false;
    let mut index = 0;
    while index < lines.len() {
        let line = normalize_whitespace(lines[index]);
        if line.is_empty() {
            index += 1;
            continue;
        }

        let upper = line.to_uppercase();
        if upper == "BUGS" || upper == "EXAMPLES" || upper == "FILES" {
            break;
        }
        if !line.starts_with('-') {
            index += 1;
            continue;
        }

        let option_line = line;
        index += 1;
        let mut desc_parts = Vec::new();

        while index < lines.len() {
            let next = normalize_whitespace(lines[index]);
            let upper = next.to_uppercase();
            if next.is_empty() {
                index += 1;
                if !desc_parts.is_empty() {
                    break;
                }
                continue;
            }
            if next.starts_with('-') || upper == "BUGS" || upper == "EXAMPLES" || upper == "FILES" {
                break;
            }
            desc_parts.push(next);
            index += 1;
        }

        found |= add_option(cmd, &option_line, &desc_parts.join(" "));
    }

    found
}

fn parse_type1(cmd: &mut Command, content: &str) -> bool {
    let mut found = false;
    if let Some(section) = extract_section(content, &["OPTIONS"]) {
        found |= parse_type1_blocks(cmd, section);
        if !found {
            found |= parse_tagged_blocks(cmd, section);
        }
    }
    found
}

fn parse_type2(cmd: &mut Command, content: &str) -> bool {
    extract_section(content, &["OPTIONS"])
        .map(|section| parse_tagged_blocks(cmd, section))
        .unwrap_or(false)
}

fn parse_type3(cmd: &mut Command, content: &str) -> bool {
    extract_section(content, &["DESCRIPTION"])
        .map(|section| parse_tagged_blocks(cmd, section))
        .unwrap_or(false)
}

fn parse_type4(cmd: &mut Command, content: &str) -> bool {
    extract_section(content, &["FUNCTION LETTERS"])
        .map(|section| parse_tagged_blocks(cmd, section))
        .unwrap_or(false)
}

fn parse_scdoc(cmd: &mut Command, content: &str) -> bool {
    if !content.contains("Generated by scdoc") {
        return false;
    }
    extract_section(content, &["OPTIONS"])
        .map(|section| parse_scdoc_blocks(cmd, section))
        .unwrap_or(false)
}

fn parse_darwin_sections(cmd: &mut Command, content: &str) -> bool {
    let mut found = false;
    if let Some(section) = extract_section(content, &["DESCRIPTION"]) {
        found |= parse_darwin(cmd, section);
    }
    if !found {
        if let Some(section) = extract_section(content, &["OPTIONS"]) {
            found |= parse_darwin(cmd, section);
        }
    }
    found
}

fn parse_subcommand_name(cmd_name: &str, token: &str) -> Option<String> {
    let normalized = normalize_whitespace(&strip_groff_wrappers(token));
    let caps = Regex::new(r"^(?P<name>[A-Za-z0-9][A-Za-z0-9+._-]*)\((?P<section>\d+)\)$")
        .unwrap()
        .captures(&normalized)?;
    let name = caps.name("name").unwrap().as_str();
    let prefix = format!("{cmd_name}-");
    let stripped = name.strip_prefix(&prefix)?;

    if stripped.is_empty() {
        return None;
    }

    Some(stripped.to_string())
}

fn add_subcommand(cmd: &mut Command, name: &str, description: &str) -> bool {
    let description = clean_sentence(&normalize_text(
        description,
        cmd.name.as_deref().unwrap_or(""),
    ));

    if let Some(existing) = cmd
        .subcommands
        .iter_mut()
        .find(|subcommand| subcommand.name.as_deref() == Some(name))
    {
        if existing
            .description
            .as_deref()
            .unwrap_or_default()
            .is_empty()
            && !description.is_empty()
        {
            existing.description = Some(description);
        }
        return false;
    }

    cmd.subcommands.push(Command {
        name: Some(name.to_string()),
        description: if description.is_empty() {
            None
        } else {
            Some(description)
        },
        args: Vec::new(),
        subcommands: Vec::new(),
        author: None,
    });
    true
}

fn extract_subcommand_candidates(section: &str, cmd_name: &str) -> Vec<(String, String)> {
    let re = Regex::new(r"(?ms)\.PP\s*(.*?)\.RS 4\s*(.*?)\.RE").unwrap();
    let mut candidates = Vec::new();

    for caps in re.captures_iter(section) {
        let raw_name = caps.get(1).unwrap().as_str();
        let raw_description = caps.get(2).unwrap().as_str();
        let Some(name) = parse_subcommand_name(cmd_name, raw_name) else {
            continue;
        };
        candidates.push((name, raw_description.to_string()));
    }

    candidates
}

fn parse_subcommands(cmd: &mut Command, content: &str) -> bool {
    let Some(cmd_name) = cmd.name.clone() else {
        return false;
    };

    let mut found = false;

    for section in top_level_sections(content) {
        let candidates = extract_subcommand_candidates(section, &cmd_name);
        if candidates.len() < 3 {
            continue;
        }

        for (name, description) in candidates {
            found |= add_subcommand(cmd, &name, &description);
        }
    }

    found
}

pub fn parse_manpage(cmd_name: &str, content: &str) -> Option<Command> {
    let mut cmd = Command {
        name: Some(cmd_name.to_string()),
        description: None,
        args: Vec::new(),
        subcommands: Vec::new(),
        author: None,
    };

    parse_subcommands(&mut cmd, content);

    let parsers: [fn(&mut Command, &str) -> bool; 7] = [
        parse_scdoc,
        parse_type1,
        parse_type2,
        parse_type4,
        parse_type3,
        parse_darwin_sections,
        parse_deroff,
    ];

    for parser in parsers {
        let before = cmd.args.len();
        let success = parser(&mut cmd, content);
        if success && cmd.args.len() > before {
            break;
        }
    }

    if cmd.args.is_empty() { None } else { Some(cmd) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const TYPE1_FIXTURE: &str = r#".TH EXAMPLE 1
.SH "OPTIONS"
.PP
.BI \-a \&,
.BI \-\^-all
.RS 4
Show hidden files and include dot entries.
.RE
.PP
.BI \-o \ output \&,
.BI \-\^-output = file
.RS 4
Write the generated report to the chosen file path.
.RE
.PP
.BI \-d [debug-file] \&,
.BI \-\^-debug [=debug-file]
.RS 4
Enable debug logging and optionally write traces to debug-file.
.RE
"#;

    const TYPE2_FIXTURE: &str = r#".TH SAMPLE 1
.SH OPTIONS
.TP
.B \-n
Number output lines before printing them.
.TP
.BI \-f \ input-file
Read input from input-file instead of stdin.
.TP
.BI \-\^-format = json
Render the output using the requested json format.
"#;

    const DARWIN_FIXTURE: &str = r#".Dd January 1 2026
.Dt SAMPLE 1
.Os
.Sh DESCRIPTION
The options are as follows:
.Bl -tag -width Ds -compact
.It Fl a
Enable agent forwarding for the current connection.
.It Fl b Ar bind_address
Bind to bind_address before opening the remote session.
.It Fl Fl verbose
Produce verbose logs for each connection phase.
.El
"#;

    const DEROFF_FIXTURE: &str = r#".TH RAW 1
.SH DESCRIPTION
-q, --quiet
Suppress normal output while still reporting errors.

-p PATH, --path PATH
Read files from PATH before applying filters.

BUGS
None documented.
"#;

    #[derive(Clone, Copy)]
    struct ExpectedArg<'a> {
        short: Option<&'a str>,
        long: Option<&'a str>,
        value_type: Option<&'a str>,
        num_args: Option<&'a str>,
        description_contains: &'a str,
    }

    fn parse_test_manpage(name: &str) -> Command {
        let content = fs::read_to_string(format!("../tests/man_pages/{name}")).unwrap();
        let cmd_name = name.split('.').next().unwrap();
        parse_manpage(cmd_name, &content).unwrap()
    }

    fn normalize_desc(desc: Option<&str>) -> String {
        normalize_whitespace(desc.unwrap_or(""))
    }

    fn assert_expected_subcommands(cmd: &Command, expected: &[(&str, &str)]) {
        assert_eq!(cmd.subcommands.len(), expected.len());
        for (name, description_contains) in expected {
            let subcommand = cmd
                .subcommands
                .iter()
                .find(|subcommand| subcommand.name.as_deref() == Some(*name))
                .unwrap();
            let description = normalize_desc(subcommand.description.as_deref());
            assert!(!description.is_empty());
            assert!(description.contains(description_contains));
        }
    }

    fn assert_contains_subcommands(cmd: &Command, expected: &[(&str, &str)]) {
        for (name, description_contains) in expected {
            let subcommand = cmd
                .subcommands
                .iter()
                .find(|subcommand| subcommand.name.as_deref() == Some(*name))
                .unwrap();
            let description = normalize_desc(subcommand.description.as_deref());
            assert!(!description.is_empty());
            assert!(description.contains(description_contains));
        }
    }

    fn find_arg<'a>(cmd: &'a Command, expected: &ExpectedArg<'_>) -> &'a Arg {
        cmd.args
            .iter()
            .find(|arg| {
                arg.short.as_deref() == expected.short && arg.long.as_deref() == expected.long
            })
            .or_else(|| {
                cmd.args.iter().find(|arg| {
                    (expected.short.is_some() && arg.short.as_deref() == expected.short)
                        || (expected.long.is_some() && arg.long.as_deref() == expected.long)
                })
            })
            .unwrap()
    }

    fn assert_expected_args(cmd: &Command, expected: &[ExpectedArg<'_>]) {
        assert_eq!(cmd.args.len(), expected.len());
        for expected_arg in expected {
            let arg = find_arg(cmd, expected_arg);
            assert_eq!(arg.short.as_deref(), expected_arg.short);
            assert_eq!(arg.long.as_deref(), expected_arg.long);
            assert_eq!(arg.value_type.as_deref(), expected_arg.value_type);
            assert_eq!(arg.num_args.as_deref(), expected_arg.num_args);
            let description = normalize_desc(arg.description.as_deref());
            assert!(!description.is_empty());
            assert!(description.contains(expected_arg.description_contains));
        }
    }

    fn assert_contains_expected_args(cmd: &Command, expected: &[ExpectedArg<'_>]) {
        for expected_arg in expected {
            let arg = find_arg(cmd, expected_arg);
            assert_eq!(arg.short.as_deref(), expected_arg.short);
            assert_eq!(arg.long.as_deref(), expected_arg.long);
            assert_eq!(arg.value_type.as_deref(), expected_arg.value_type);
            assert_eq!(arg.num_args.as_deref(), expected_arg.num_args);
            let description = normalize_desc(arg.description.as_deref());
            assert!(!description.is_empty());
            assert!(description.contains(expected_arg.description_contains));
        }
    }

    #[test]
    fn parses_type1_options_exhaustively() {
        let cmd = parse_manpage("example", TYPE1_FIXTURE).unwrap();
        assert_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    short: Some("-a"),
                    long: Some("--all"),
                    value_type: None,
                    num_args: None,
                    description_contains: "Show hidden files",
                },
                ExpectedArg {
                    short: Some("-o"),
                    long: Some("--output"),
                    value_type: Some("file"),
                    num_args: Some("1"),
                    description_contains: "chosen file path",
                },
                ExpectedArg {
                    short: Some("-d"),
                    long: Some("--debug"),
                    value_type: Some("debug-file"),
                    num_args: Some("?"),
                    description_contains: "optionally write traces",
                },
            ],
        );
    }

    #[test]
    fn parses_type2_options_exhaustively() {
        let cmd = parse_manpage("sample", TYPE2_FIXTURE).unwrap();
        assert_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    short: Some("-n"),
                    long: None,
                    value_type: None,
                    num_args: None,
                    description_contains: "Number output lines",
                },
                ExpectedArg {
                    short: Some("-f"),
                    long: None,
                    value_type: Some("input-file"),
                    num_args: Some("1"),
                    description_contains: "instead of stdin",
                },
                ExpectedArg {
                    short: None,
                    long: Some("--format"),
                    value_type: Some("json"),
                    num_args: Some("1"),
                    description_contains: "requested json format",
                },
            ],
        );
    }

    #[test]
    fn parses_darwin_options_exhaustively() {
        let cmd = parse_manpage("sample", DARWIN_FIXTURE).unwrap();
        assert_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    short: Some("-a"),
                    long: None,
                    value_type: None,
                    num_args: None,
                    description_contains: "agent forwarding",
                },
                ExpectedArg {
                    short: Some("-b"),
                    long: None,
                    value_type: Some("bind_address"),
                    num_args: Some("1"),
                    description_contains: "before opening the remote session",
                },
                ExpectedArg {
                    short: None,
                    long: Some("--verbose"),
                    value_type: None,
                    num_args: None,
                    description_contains: "verbose logs",
                },
            ],
        );
    }

    #[test]
    fn parses_deroff_options_exhaustively() {
        let cmd = parse_manpage("raw", DEROFF_FIXTURE).unwrap();
        assert_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    short: Some("-q"),
                    long: Some("--quiet"),
                    value_type: None,
                    num_args: None,
                    description_contains: "Suppress normal output",
                },
                ExpectedArg {
                    short: Some("-p"),
                    long: Some("--path"),
                    value_type: Some("PATH"),
                    num_args: Some("1"),
                    description_contains: "Read files from PATH",
                },
            ],
        );
    }

    #[test]
    fn parses_real_git_options_with_values_and_descriptions() {
        let cmd = parse_test_manpage("git.1");
        assert!(cmd.subcommands.len() >= 9);
        assert_contains_subcommands(
            &cmd,
            &[
                ("add", "file contents to the index"),
                ("commit", "Record changes to the repository"),
                ("diff", "Show changes between commits"),
                ("fetch", "Download objects and refs"),
                ("init", "Create an empty Git repository"),
                ("log", "Show commit logs"),
                ("pull", "Fetch from and integrate"),
                ("push", "Update remote refs"),
                ("status", "Show the working tree status"),
            ],
        );
        let expected = [
            ExpectedArg {
                short: Some("-v"),
                long: Some("--version"),
                value_type: None,
                num_args: None,
                description_contains: "Prints the Git suite version",
            },
            ExpectedArg {
                short: Some("-C"),
                long: None,
                value_type: Some("<path>"),
                num_args: Some("1"),
                description_contains: "instead of the current working directory",
            },
            ExpectedArg {
                short: Some("-c"),
                long: None,
                value_type: Some("<name>=<value>"),
                num_args: Some("1"),
                description_contains: "override values from configuration files",
            },
            ExpectedArg {
                short: None,
                long: Some("--config-env"),
                value_type: Some("<name>=<envvar>"),
                num_args: Some("1"),
                description_contains: "retrieve the value",
            },
        ];

        for item in expected {
            let arg = find_arg(&cmd, &item);
            assert_eq!(arg.short.as_deref(), item.short);
            assert_eq!(arg.long.as_deref(), item.long);
            assert_eq!(arg.value_type.as_deref(), item.value_type);
            assert_eq!(arg.num_args.as_deref(), item.num_args);
            assert!(normalize_desc(arg.description.as_deref()).contains(item.description_contains));
        }
    }

    #[test]
    fn parses_real_cat_fixture() {
        let cmd = parse_test_manpage("cat.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    short: Some("-A"),
                    long: Some("--show-all"),
                    value_type: None,
                    num_args: None,
                    description_contains: "equivalent to -vET",
                },
                ExpectedArg {
                    short: Some("-b"),
                    long: Some("--number-nonblank"),
                    value_type: None,
                    num_args: None,
                    description_contains: "number nonempty output lines",
                },
                ExpectedArg {
                    short: Some("-u"),
                    long: None,
                    value_type: None,
                    num_args: None,
                    description_contains: "(ignored)",
                },
                ExpectedArg {
                    short: None,
                    long: Some("--help"),
                    value_type: None,
                    num_args: None,
                    description_contains: "display this help and exit",
                },
            ],
        );
    }

    #[test]
    fn parses_real_chmod_fixture() {
        let cmd = parse_test_manpage("chmod.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    short: Some("-c"),
                    long: Some("--changes"),
                    value_type: None,
                    num_args: None,
                    description_contains: "report only when a change is made",
                },
                ExpectedArg {
                    short: Some("-v"),
                    long: Some("--verbose"),
                    value_type: None,
                    num_args: None,
                    description_contains: "diagnostic for every file processed",
                },
                ExpectedArg {
                    short: None,
                    long: Some("--reference"),
                    value_type: Some("RFILE"),
                    num_args: Some("1"),
                    description_contains: "use RFILE's mode",
                },
                ExpectedArg {
                    short: Some("-R"),
                    long: Some("--recursive"),
                    value_type: None,
                    num_args: None,
                    description_contains: "change files and directories recursively",
                },
            ],
        );
    }

    #[test]
    fn parses_real_chown_fixture() {
        let cmd = parse_test_manpage("chown.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    short: Some("-c"),
                    long: Some("--changes"),
                    value_type: None,
                    num_args: None,
                    description_contains: "report only when a change is made",
                },
                ExpectedArg {
                    short: Some("-h"),
                    long: Some("--no-dereference"),
                    value_type: None,
                    num_args: None,
                    description_contains: "affect symbolic links instead of any referenced file",
                },
                ExpectedArg {
                    short: None,
                    long: Some("--from"),
                    value_type: Some("CURRENT_OWNER:CURRENT_GROUP"),
                    num_args: Some("1"),
                    description_contains: "only if its current owner and/or group match",
                },
                ExpectedArg {
                    short: None,
                    long: Some("--reference"),
                    value_type: Some("RFILE"),
                    num_args: Some("1"),
                    description_contains: "use RFILE's owner and group",
                },
            ],
        );
    }

    #[test]
    fn parses_real_cp_fixture() {
        let cmd = parse_test_manpage("cp.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    short: Some("-a"),
                    long: Some("--archive"),
                    value_type: None,
                    num_args: None,
                    description_contains: "same as -dR --preserve=all",
                },
                ExpectedArg {
                    short: Some("-b"),
                    long: None,
                    value_type: None,
                    num_args: None,
                    description_contains: "does not accept an argument",
                },
                ExpectedArg {
                    short: None,
                    long: Some("--attributes-only"),
                    value_type: None,
                    num_args: None,
                    description_contains: "don't copy the file data",
                },
                ExpectedArg {
                    short: Some("-S"),
                    long: Some("--suffix"),
                    value_type: Some("SUFFIX"),
                    num_args: Some("1"),
                    description_contains: "override the usual backup suffix",
                },
            ],
        );
    }

    #[test]
    fn parses_real_curl_fixture() {
        let cmd = parse_test_manpage("curl.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    short: Some("-g"),
                    long: Some("--globoff"),
                    value_type: None,
                    num_args: None,
                    description_contains: "URL globbing parser",
                },
                ExpectedArg {
                    short: Some("-o"),
                    long: Some("--output"),
                    value_type: Some("<file>"),
                    num_args: Some("1"),
                    description_contains: "Write output to <file>",
                },
                ExpectedArg {
                    short: None,
                    long: Some("--abstract-unix-socket"),
                    value_type: Some("<path>"),
                    num_args: Some("1"),
                    description_contains: "Connect through an abstract Unix domain socket",
                },
                ExpectedArg {
                    short: Some("-s"),
                    long: Some("--silent"),
                    value_type: None,
                    num_args: None,
                    description_contains: "Silent or quiet mode",
                },
            ],
        );
    }

    #[test]
    fn parses_real_gawk_fixture() {
        let cmd = parse_test_manpage("gawk.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    short: Some("-f"),
                    long: Some("--file"),
                    value_type: Some("program-file"),
                    num_args: Some("1"),
                    description_contains: "program source from the file",
                },
                ExpectedArg {
                    short: Some("-F"),
                    long: Some("--field-separator"),
                    value_type: Some("fs"),
                    num_args: Some("1"),
                    description_contains: "input field separator",
                },
                ExpectedArg {
                    short: Some("-b"),
                    long: Some("--characters-as-bytes"),
                    value_type: None,
                    num_args: None,
                    description_contains: "single-byte characters",
                },
                ExpectedArg {
                    short: Some("-c"),
                    long: Some("--traditional"),
                    value_type: None,
                    num_args: None,
                    description_contains: "compatibility mode",
                },
            ],
        );
    }

    #[test]
    fn parses_real_grep_fixture() {
        let cmd = parse_test_manpage("grep.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    short: Some("-E"),
                    long: Some("--extended-regexp"),
                    value_type: None,
                    num_args: None,
                    description_contains: "extended regular expressions",
                },
                ExpectedArg {
                    short: Some("-i"),
                    long: Some("--ignore-case"),
                    value_type: None,
                    num_args: None,
                    description_contains: "Ignore case distinctions",
                },
                ExpectedArg {
                    short: Some("-f"),
                    long: Some("--file"),
                    value_type: Some("FILE"),
                    num_args: Some("1"),
                    description_contains: "Obtain patterns from FILE",
                },
                ExpectedArg {
                    short: None,
                    long: Some("--no-ignore-case"),
                    value_type: None,
                    num_args: None,
                    description_contains: "Do not ignore case distinctions",
                },
            ],
        );
    }

    #[test]
    fn parses_real_ls_fixture() {
        let cmd = parse_test_manpage("ls.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    short: Some("-a"),
                    long: Some("--all"),
                    value_type: None,
                    num_args: None,
                    description_contains: "do not ignore entries starting with",
                },
                ExpectedArg {
                    short: None,
                    long: Some("--author"),
                    value_type: None,
                    num_args: None,
                    description_contains: "print the author of each file",
                },
                ExpectedArg {
                    short: None,
                    long: Some("--block-size"),
                    value_type: Some("SIZE"),
                    num_args: Some("1"),
                    description_contains: "scale sizes by SIZE",
                },
                ExpectedArg {
                    short: Some("-d"),
                    long: Some("--directory"),
                    value_type: None,
                    num_args: None,
                    description_contains: "list directories themselves",
                },
            ],
        );
    }

    #[test]
    fn parses_real_mkdir_fixture() {
        let cmd = parse_test_manpage("mkdir.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    short: Some("-m"),
                    long: Some("--mode"),
                    value_type: Some("MODE"),
                    num_args: Some("1"),
                    description_contains: "set file mode",
                },
                ExpectedArg {
                    short: Some("-p"),
                    long: Some("--parents"),
                    value_type: None,
                    num_args: None,
                    description_contains: "make parent directories as needed",
                },
                ExpectedArg {
                    short: Some("-v"),
                    long: Some("--verbose"),
                    value_type: None,
                    num_args: None,
                    description_contains: "message for each created directory",
                },
                ExpectedArg {
                    short: None,
                    long: Some("--help"),
                    value_type: None,
                    num_args: None,
                    description_contains: "display this help and exit",
                },
            ],
        );
    }

    #[test]
    fn parses_real_mv_fixture() {
        let cmd = parse_test_manpage("mv.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    short: Some("-b"),
                    long: None,
                    value_type: None,
                    num_args: None,
                    description_contains: "does not accept an argument",
                },
                ExpectedArg {
                    short: Some("-f"),
                    long: Some("--force"),
                    value_type: None,
                    num_args: None,
                    description_contains: "do not prompt before overwriting",
                },
                ExpectedArg {
                    short: Some("-S"),
                    long: Some("--suffix"),
                    value_type: Some("SUFFIX"),
                    num_args: Some("1"),
                    description_contains: "override the usual backup suffix",
                },
                ExpectedArg {
                    short: Some("-t"),
                    long: Some("--target-directory"),
                    value_type: Some("DIRECTORY"),
                    num_args: Some("1"),
                    description_contains: "move all SOURCE arguments into DIRECTORY",
                },
            ],
        );
    }

    #[test]
    fn parses_real_ping_fixture() {
        let cmd = parse_test_manpage("ping.8");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    short: Some("-4"),
                    long: None,
                    value_type: None,
                    num_args: None,
                    description_contains: "Use IPv4 only",
                },
                ExpectedArg {
                    short: Some("-6"),
                    long: None,
                    value_type: None,
                    num_args: None,
                    description_contains: "Use IPv6 only",
                },
                ExpectedArg {
                    short: Some("-a"),
                    long: None,
                    value_type: None,
                    num_args: None,
                    description_contains: "Audible ping",
                },
                ExpectedArg {
                    short: Some("-c"),
                    long: None,
                    value_type: Some("count"),
                    num_args: Some("1"),
                    description_contains: "Stop after sending count",
                },
            ],
        );
    }

    #[test]
    fn parses_real_rm_fixture() {
        let cmd = parse_test_manpage("rm.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    short: Some("-f"),
                    long: Some("--force"),
                    value_type: None,
                    num_args: None,
                    description_contains: "ignore nonexistent files and arguments",
                },
                ExpectedArg {
                    short: Some("-i"),
                    long: None,
                    value_type: None,
                    num_args: None,
                    description_contains: "prompt before every removal",
                },
                ExpectedArg {
                    short: None,
                    long: Some("--interactive"),
                    value_type: Some("WHEN"),
                    num_args: Some("?"),
                    description_contains: "prompt according to WHEN",
                },
                ExpectedArg {
                    short: Some("-R"),
                    long: Some("--recursive"),
                    value_type: None,
                    num_args: None,
                    description_contains: "remove directories and their contents recursively",
                },
            ],
        );
    }

    #[test]
    fn parses_real_sed_fixture() {
        let cmd = parse_test_manpage("sed.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    short: Some("-n"),
                    long: Some("--quiet"),
                    value_type: None,
                    num_args: None,
                    description_contains: "suppress automatic printing of pattern space",
                },
                ExpectedArg {
                    short: Some("-e"),
                    long: Some("--expression"),
                    value_type: Some("script"),
                    num_args: Some("1"),
                    description_contains: "add the script to the commands",
                },
                ExpectedArg {
                    short: Some("-i"),
                    long: Some("--in-place"),
                    value_type: Some("SUFFIX"),
                    num_args: Some("?"),
                    description_contains: "edit files in place",
                },
                ExpectedArg {
                    short: Some("-u"),
                    long: Some("--unbuffered"),
                    value_type: None,
                    num_args: None,
                    description_contains: "load minimal amounts of data",
                },
            ],
        );
    }

    #[test]
    fn parses_real_tar_fixture() {
        let cmd = parse_test_manpage("tar.1");
        assert_expected_subcommands(&cmd, &[]);
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    short: Some("-a"),
                    long: Some("--auto-compress"),
                    value_type: None,
                    num_args: None,
                    description_contains: "compression program",
                },
                ExpectedArg {
                    short: Some("-f"),
                    long: Some("--file"),
                    value_type: Some("ARCHIVE"),
                    num_args: Some("1"),
                    description_contains: "archive file or device ARCHIVE",
                },
                ExpectedArg {
                    short: Some("-v"),
                    long: Some("--verbose"),
                    value_type: None,
                    num_args: None,
                    description_contains: "files processed",
                },
                ExpectedArg {
                    short: Some("-V"),
                    long: Some("--label"),
                    value_type: Some("TEXT"),
                    num_args: Some("1"),
                    description_contains: "volume name TEXT",
                },
            ],
        );
    }

    #[test]
    fn parses_real_wget_fixture() {
        let cmd = parse_test_manpage("wget.1");
        assert_expected_subcommands(&cmd, &[]);
        assert!(
            cmd.args
                .iter()
                .all(|arg| arg.long.as_deref() != Some("--no-"))
        );
        assert_contains_expected_args(
            &cmd,
            &[
                ExpectedArg {
                    short: Some("-V"),
                    long: Some("--version"),
                    value_type: None,
                    num_args: None,
                    description_contains: "Display the version of Wget",
                },
                ExpectedArg {
                    short: Some("-b"),
                    long: Some("--background"),
                    value_type: None,
                    num_args: None,
                    description_contains: "Go to background immediately after startup",
                },
                ExpectedArg {
                    short: Some("-o"),
                    long: Some("--output-file"),
                    value_type: Some("logfile"),
                    num_args: Some("1"),
                    description_contains: "Log all messages to logfile",
                },
                ExpectedArg {
                    short: None,
                    long: Some("--report-speed"),
                    value_type: Some("type"),
                    num_args: Some("1"),
                    description_contains: "Output bandwidth as type",
                },
                ExpectedArg {
                    short: None,
                    long: Some("--load-cookies"),
                    value_type: Some("file"),
                    num_args: Some("1"),
                    description_contains: "Load cookies from file before the first HTTP retrieval",
                },
            ],
        );
    }

    #[test]
    fn parses_real_find_options_with_descriptions() {
        let cmd = parse_test_manpage("find.1");
        assert_expected_subcommands(&cmd, &[]);
        for item in [
            ExpectedArg {
                short: Some("-P"),
                long: None,
                value_type: None,
                num_args: None,
                description_contains: "Never follow symbolic links",
            },
            ExpectedArg {
                short: Some("-L"),
                long: None,
                value_type: None,
                num_args: None,
                description_contains: "Follow symbolic links",
            },
            ExpectedArg {
                short: Some("-H"),
                long: None,
                value_type: None,
                num_args: None,
                description_contains: "except while processing the command line arguments",
            },
        ] {
            let arg = find_arg(&cmd, &item);
            assert_eq!(arg.short.as_deref(), item.short);
            assert!(normalize_desc(arg.description.as_deref()).contains(item.description_contains));
        }
    }

    #[test]
    fn parses_real_ssh_options_with_values_and_descriptions() {
        let cmd = parse_test_manpage("ssh.1");
        assert_expected_subcommands(&cmd, &[]);
        for item in [
            ExpectedArg {
                short: Some("-4"),
                long: None,
                value_type: None,
                num_args: None,
                description_contains: "IPv4 addresses only",
            },
            ExpectedArg {
                short: Some("-B"),
                long: None,
                value_type: Some("bind_interface"),
                num_args: Some("1"),
                description_contains: "Bind to the address",
            },
            ExpectedArg {
                short: Some("-b"),
                long: None,
                value_type: Some("bind_address"),
                num_args: Some("1"),
                description_contains: "source address",
            },
        ] {
            let arg = find_arg(&cmd, &item);
            assert_eq!(arg.short.as_deref(), item.short);
            assert_eq!(arg.value_type.as_deref(), item.value_type);
            assert!(normalize_desc(arg.description.as_deref()).contains(item.description_contains));
        }
    }

    #[test]
    fn parses_real_sudo_options_with_short_long_pairs() {
        let cmd = parse_test_manpage("sudo.8");
        assert_expected_subcommands(&cmd, &[]);
        for item in [
            ExpectedArg {
                short: Some("-A"),
                long: Some("--askpass"),
                value_type: None,
                num_args: None,
                description_contains: "requires a password",
            },
            ExpectedArg {
                short: Some("-a"),
                long: Some("--auth-type"),
                value_type: Some("type"),
                num_args: Some("1"),
                description_contains: "authentication",
            },
        ] {
            let arg = find_arg(&cmd, &item);
            assert_eq!(arg.short.as_deref(), item.short);
            assert_eq!(arg.long.as_deref(), item.long);
            assert_eq!(arg.value_type.as_deref(), item.value_type);
            assert!(normalize_desc(arg.description.as_deref()).contains(item.description_contains));
        }
    }

    #[test]
    fn parses_real_zstd_fixture() {
        let cmd = parse_test_manpage("zstd.1");
        println!("PARSED ARGS: {:#?}", cmd.args);

        // Assertions on the parsed command structure and options
        let keep_item = ExpectedArg {
            short: Some("-k"),
            long: Some("--keep"),
            value_type: None,
            num_args: None,
            description_contains: "keep source file(s)",
        };
        let keep_arg = find_arg(&cmd, &keep_item);
        assert_eq!(keep_arg.short.as_deref(), Some("-k"));
        assert_eq!(keep_arg.long.as_deref(), Some("--keep"));
        assert!(normalize_desc(keep_arg.description.as_deref()).contains("keep source file(s)"));

        let rm_item = ExpectedArg {
            short: None,
            long: Some("--rm"),
            value_type: None,
            num_args: None,
            description_contains: "remove source file(s)",
        };
        let rm_arg = find_arg(&cmd, &rm_item);
        assert_eq!(rm_arg.long.as_deref(), Some("--rm"));

        let decompress_item = ExpectedArg {
            short: Some("-d"),
            long: Some("--decompress"),
            value_type: None,
            num_args: None,
            description_contains: "Decompress",
        };
        let decompress_arg = find_arg(&cmd, &decompress_item);
        assert_eq!(decompress_arg.short.as_deref(), Some("-d"));

        let ultra_item = ExpectedArg {
            short: None,
            long: Some("--ultra"),
            value_type: None,
            num_args: None,
            description_contains: "unlocks high compression levels",
        };
        let ultra_arg = find_arg(&cmd, &ultra_item);
        assert_eq!(ultra_arg.long.as_deref(), Some("--ultra"));
    }
}
