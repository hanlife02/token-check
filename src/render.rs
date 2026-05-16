use std::env;
use terminal_size::{terminal_size, Width};

const DEFAULT_TERMINAL_WIDTH: usize = 100;
const ANSI_RESET: &str = "\x1b[0m";
const ANSI_CYAN: &str = "\x1b[36m";
const ANSI_BOLD_WHITE: &str = "\x1b[1;37m";
const ANSI_BOLD_YELLOW: &str = "\x1b[1;33m";
const ANSI_DIM: &str = "\x1b[2m";

pub(crate) fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    let mut widths = headers
        .iter()
        .map(|header| display_width(header))
        .collect::<Vec<_>>();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(display_width(cell));
        }
    }

    print_row(
        headers.iter().map(|cell| cell.to_string()).collect(),
        &widths,
    );
    let separator = widths
        .iter()
        .map(|width| "-".repeat(*width))
        .collect::<Vec<_>>();
    print_row(separator, &widths);
    for row in rows {
        print_row(row.clone(), &widths);
    }
}

fn print_row(row: Vec<String>, widths: &[usize]) {
    for (index, cell) in row.iter().enumerate() {
        if index > 0 {
            print!("  ");
        }
        let padding = widths[index].saturating_sub(display_width(cell));
        print!("{cell}{}", " ".repeat(padding));
    }
    println!();
}

pub(crate) fn display_width(value: &str) -> usize {
    let mut width = 0;
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for sequence_ch in chars.by_ref() {
                if sequence_ch.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            width += char_display_width(ch);
        }
    }
    width
}

fn char_display_width(ch: char) -> usize {
    if ch.is_control() {
        return 0;
    }
    let code = ch as u32;
    if matches!(
        code,
        0x1100..=0x115F
            | 0x2329..=0x232A
            | 0x2E80..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFE10..=0xFE19
            | 0xFE30..=0xFE6F
            | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6
    ) {
        2
    } else {
        1
    }
}

pub(crate) fn center_visible(value: &str, width: usize) -> String {
    let value_width = display_width(value);
    if value_width >= width {
        return value.to_string();
    }
    let padding = width - value_width;
    let left = padding / 2;
    let right = padding - left;
    format!("{}{}{}", " ".repeat(left), value, " ".repeat(right))
}

fn pad_right_visible(value: &str, width: usize) -> String {
    format!(
        "{}{}",
        value,
        " ".repeat(width.saturating_sub(display_width(value)))
    )
}

pub(crate) fn print_rounded_panel(title: &str, lines: &[String]) {
    let panel_width = terminal_panel_width();
    let inner_limit = panel_width.saturating_sub(4);
    let title_width = display_width(title) + 2;
    let inner_width = lines
        .iter()
        .map(|line| display_width(line))
        .max()
        .unwrap_or(0)
        .max(title_width + 1)
        .min(inner_limit);
    let border_width = inner_width + 2;
    let top_width = border_width + 2;
    let title_label = if top_width > 6 {
        truncate_visible(&format!(" {title} "), top_width.saturating_sub(3))
    } else {
        String::new()
    };
    let title_label_width = display_width(&title_label);
    let fill_width = top_width.saturating_sub(title_label_width + 3);

    println!(
        "{ANSI_CYAN}╭─{ANSI_BOLD_WHITE}{title_label}{ANSI_RESET}{ANSI_CYAN}{}╮{ANSI_RESET}",
        "─".repeat(fill_width)
    );
    for line in lines {
        let line = truncate_visible(line, inner_width);
        println!(
            "{ANSI_CYAN}│{ANSI_RESET} {} {ANSI_CYAN}│{ANSI_RESET}",
            pad_right_visible(&line, inner_width)
        );
    }
    println!("{ANSI_CYAN}╰{}╯{ANSI_RESET}", "─".repeat(border_width));
}

pub(crate) fn truncate_visible(value: &str, width: usize) -> String {
    if display_width(value) <= width {
        return value.to_string();
    }

    let mut out = String::new();
    let mut visible_width = 0;
    let mut saw_escape = false;
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            saw_escape = true;
            out.push(ch);
            out.push(chars.next().unwrap_or('['));
            for sequence_ch in chars.by_ref() {
                out.push(sequence_ch);
                if sequence_ch.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        if visible_width >= width {
            break;
        }
        let ch_width = char_display_width(ch);
        if visible_width + ch_width > width {
            break;
        }
        out.push(ch);
        visible_width += ch_width;
    }
    if saw_escape {
        out.push_str(ANSI_RESET);
    }
    out
}

pub(crate) fn terminal_panel_width() -> usize {
    detected_terminal_width()
        .unwrap_or(DEFAULT_TERMINAL_WIDTH)
        .saturating_sub(1)
        .max(4)
}

fn detected_terminal_width() -> Option<usize> {
    env::var("COLUMNS")
        .ok()
        .and_then(|value| parse_terminal_width(&value))
        .or_else(native_terminal_width)
}

fn native_terminal_width() -> Option<usize> {
    let (Width(width), _) = terminal_size()?;
    Some(usize::from(width))
}

pub(crate) fn parse_terminal_width(value: &str) -> Option<usize> {
    value
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|width| *width > 0)
}

pub(crate) fn bold_yellow(value: &str) -> String {
    format!("{ANSI_BOLD_YELLOW}{value}{ANSI_RESET}")
}

pub(crate) fn dim(value: &str) -> String {
    format!("{ANSI_DIM}{value}{ANSI_RESET}")
}

#[cfg(test)]
mod tests {
    use super::{display_width, parse_terminal_width, truncate_visible};

    #[test]
    fn measures_ansi_styled_display_width() {
        assert_eq!(display_width("\x1b[38;5;46m██\x1b[0m"), 2);
        assert_eq!(display_width("\x1b[2mLess\x1b[0m"), 4);
        assert_eq!(display_width("配置"), 4);
    }

    #[test]
    fn truncates_ansi_styled_text_by_visible_width() {
        let value = truncate_visible("\x1b[2mabcdef\x1b[0m", 3);
        assert_eq!(display_width(&value), 3);
        assert!(value.ends_with("\x1b[0m"));

        let value = truncate_visible("配置文件", 5);
        assert_eq!(value, "配置");
    }

    #[test]
    fn parses_terminal_width_values() {
        assert_eq!(parse_terminal_width("80\n"), Some(80));
        assert_eq!(parse_terminal_width("0"), None);
        assert_eq!(parse_terminal_width("wide"), None);
    }
}
