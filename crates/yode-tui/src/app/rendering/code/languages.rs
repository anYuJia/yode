use super::{CodeToken, CodeTokenKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommentStyle {
    Hash,
    SlashSlash,
    HashOrSlashSlash,
    ShellHash,
}

#[derive(Clone, Copy)]
pub(super) struct SyntaxProfile {
    pub(super) comment_style: CommentStyle,
    pub(super) recognize_at_decorator: bool,
    pub(super) recognize_rust_attribute: bool,
    pub(super) recognize_variables: bool,
    pub(super) keyword_matcher: fn(&str) -> bool,
}

pub(super) fn tokenize_diff_line(line: &str) -> Vec<CodeToken> {
    let (indent, content) = split_leading_whitespace(line);
    let mut tokens = Vec::new();
    if !indent.is_empty() {
        tokens.push(CodeToken {
            text: indent.to_string(),
            kind: CodeTokenKind::Plain,
        });
    }

    if content.starts_with("@@") {
        tokenize_diff_hunk_line(content, &mut tokens);
    } else if let Some((prefix, path)) = diff_file_header_parts(content) {
        tokens.push(CodeToken {
            text: prefix.to_string(),
            kind: CodeTokenKind::DiffMeta,
        });
        if !path.is_empty() {
            tokens.push(CodeToken {
                text: path.to_string(),
                kind: CodeTokenKind::DiffFile,
            });
        }
    } else {
        let kind = if content.starts_with("diff ") || content.starts_with("index ") {
            CodeTokenKind::DiffMeta
        } else if content.starts_with('+') {
            CodeTokenKind::DiffAdded
        } else if content.starts_with('-') {
            CodeTokenKind::DiffRemoved
        } else {
            CodeTokenKind::Plain
        };

        tokens.push(CodeToken {
            text: content.to_string(),
            kind,
        });
    }
    tokens
}

fn diff_file_header_parts(content: &str) -> Option<(&str, &str)> {
    const PREFIXES: [&str; 7] = [
        "diff --git ",
        "+++ ",
        "--- ",
        "rename from ",
        "rename to ",
        "copy from ",
        "copy to ",
    ];

    PREFIXES
        .into_iter()
        .find_map(|prefix| content.strip_prefix(prefix).map(|rest| (prefix, rest)))
}

fn tokenize_diff_hunk_line(content: &str, tokens: &mut Vec<CodeToken>) {
    if let Some(end) = content[2..].find("@@") {
        let middle_end = 2 + end;
        tokens.push(CodeToken {
            text: "@@".to_string(),
            kind: CodeTokenKind::DiffHunk,
        });
        if middle_end > 2 {
            let middle = &content[2..middle_end];
            push_diff_hunk_ranges(middle, tokens);
        }
        tokens.push(CodeToken {
            text: "@@".to_string(),
            kind: CodeTokenKind::DiffHunk,
        });
        if middle_end + 2 < content.len() {
            tokens.push(CodeToken {
                text: content[middle_end + 2..].to_string(),
                kind: CodeTokenKind::Plain,
            });
        }
    } else {
        tokens.push(CodeToken {
            text: content.to_string(),
            kind: CodeTokenKind::DiffHunk,
        });
    }
}

fn push_diff_hunk_ranges(text: &str, tokens: &mut Vec<CodeToken>) {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut index = 0;

    while index < len {
        if chars[index].is_whitespace() {
            let start = index;
            while index < len && chars[index].is_whitespace() {
                index += 1;
            }
            tokens.push(CodeToken {
                text: chars[start..index].iter().collect(),
                kind: CodeTokenKind::Plain,
            });
            continue;
        }

        if matches!(chars[index], '+' | '-') {
            let start = index;
            index += 1;
            while index < len && (chars[index].is_ascii_digit() || chars[index] == ',') {
                index += 1;
            }
            tokens.push(CodeToken {
                text: chars[start..index].iter().collect(),
                kind: CodeTokenKind::DiffLineNumber,
            });
            continue;
        }

        let start = index;
        while index < len && !chars[index].is_whitespace() {
            index += 1;
        }
        tokens.push(CodeToken {
            text: chars[start..index].iter().collect(),
            kind: CodeTokenKind::Plain,
        });
    }
}

pub(super) fn tokenize_json_line(line: &str) -> Vec<CodeToken> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut index = 0;

    while index < len {
        if chars[index] == '"' {
            let start = index;
            index = consume_quoted_string(&chars, index);
            let kind = if next_non_whitespace(&chars, index) == Some(':') {
                CodeTokenKind::Property
            } else {
                CodeTokenKind::String
            };
            tokens.push(CodeToken {
                text: chars[start..index.min(len)].iter().collect(),
                kind,
            });
            continue;
        }

        if chars[index].is_ascii_digit()
            || (chars[index] == '-' && index + 1 < len && chars[index + 1].is_ascii_digit())
        {
            let start = index;
            index += 1;
            while index < len
                && (chars[index].is_ascii_digit()
                    || matches!(chars[index], '.' | 'e' | 'E' | '+' | '-'))
            {
                index += 1;
            }
            tokens.push(CodeToken {
                text: chars[start..index].iter().collect(),
                kind: CodeTokenKind::Number,
            });
            continue;
        }

        if chars[index].is_alphabetic() {
            let start = index;
            while index < len && chars[index].is_alphabetic() {
                index += 1;
            }
            let word: String = chars[start..index].iter().collect();
            tokens.push(CodeToken {
                text: word.clone(),
                kind: if matches!(word.as_str(), "true" | "false" | "null") {
                    CodeTokenKind::Keyword
                } else {
                    CodeTokenKind::Plain
                },
            });
            continue;
        }

        if matches!(chars[index], '{' | '}' | '[' | ']' | ':' | ',') {
            tokens.push(CodeToken {
                text: chars[index].to_string(),
                kind: CodeTokenKind::Operator,
            });
            index += 1;
            continue;
        }

        tokens.push(CodeToken {
            text: chars[index].to_string(),
            kind: CodeTokenKind::Plain,
        });
        index += 1;
    }

    tokens
}

pub(super) fn tokenize_generic_line(line: &str, profile: SyntaxProfile) -> Vec<CodeToken> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut index = 0;

    while index < len {
        if profile.recognize_rust_attribute
            && chars[index] == '#'
            && index + 1 < len
            && chars[index + 1] == '['
        {
            let start = index;
            index += 2;
            let mut depth = 1;
            while index < len && depth > 0 {
                match chars[index] {
                    '[' => depth += 1,
                    ']' => depth -= 1,
                    _ => {}
                }
                index += 1;
            }
            tokens.push(CodeToken {
                text: chars[start..index.min(len)].iter().collect(),
                kind: CodeTokenKind::Decorator,
            });
            continue;
        }

        if profile.recognize_at_decorator && chars[index] == '@' {
            let start = index;
            index += 1;
            while index < len && is_word_char(chars[index]) {
                index += 1;
            }
            tokens.push(CodeToken {
                text: chars[start..index].iter().collect(),
                kind: CodeTokenKind::Decorator,
            });
            continue;
        }

        if profile.recognize_variables && chars[index] == '$' {
            let start = index;
            index += 1;
            if index < len && chars[index] == '{' {
                index += 1;
                while index < len && chars[index] != '}' {
                    index += 1;
                }
                if index < len {
                    index += 1;
                }
                tokens.push(CodeToken {
                    text: chars[start..index.min(len)].iter().collect(),
                    kind: CodeTokenKind::Variable,
                });
                continue;
            }

            if index < len
                && (is_word_start(chars[index])
                    || chars[index].is_ascii_digit()
                    || matches!(chars[index], '?' | '!' | '@' | '*' | '#' | '$'))
            {
                while index < len && (is_word_char(chars[index]) || chars[index].is_ascii_digit()) {
                    index += 1;
                }
                tokens.push(CodeToken {
                    text: chars[start..index].iter().collect(),
                    kind: CodeTokenKind::Variable,
                });
                continue;
            }

            tokens.push(CodeToken {
                text: "$".to_string(),
                kind: CodeTokenKind::Operator,
            });
            continue;
        }

        if matches!(chars[index], '"' | '\'' | '`') {
            let start = index;
            index = consume_quoted_string(&chars, index);
            tokens.push(CodeToken {
                text: chars[start..index.min(len)].iter().collect(),
                kind: CodeTokenKind::String,
            });
            continue;
        }

        if starts_comment(&chars, index, profile.comment_style) {
            tokens.push(CodeToken {
                text: chars[index..].iter().collect(),
                kind: CodeTokenKind::Comment,
            });
            break;
        }

        if is_word_start(chars[index]) {
            let start = index;
            while index < len && is_word_char(chars[index]) {
                index += 1;
            }
            let word: String = chars[start..index].iter().collect();
            tokens.push(CodeToken {
                text: word.clone(),
                kind: if (profile.keyword_matcher)(&word) {
                    CodeTokenKind::Keyword
                } else {
                    CodeTokenKind::Plain
                },
            });
            continue;
        }

        if chars[index].is_ascii_digit() {
            let start = index;
            index += 1;
            while index < len
                && (chars[index].is_ascii_digit()
                    || matches!(chars[index], '.' | 'x' | 'X' | '_' | 'a'..='f' | 'A'..='F'))
            {
                index += 1;
            }
            tokens.push(CodeToken {
                text: chars[start..index].iter().collect(),
                kind: CodeTokenKind::Number,
            });
            continue;
        }

        if matches!(
            chars[index],
            '=' | '+'
                | '-'
                | '*'
                | '/'
                | '!'
                | '<'
                | '>'
                | '|'
                | '&'
                | ':'
                | '%'
                | '.'
                | ','
                | ';'
                | '('
                | ')'
                | '{'
                | '}'
                | '['
                | ']'
        ) {
            tokens.push(CodeToken {
                text: chars[index].to_string(),
                kind: CodeTokenKind::Operator,
            });
            index += 1;
            continue;
        }

        tokens.push(CodeToken {
            text: chars[index].to_string(),
            kind: CodeTokenKind::Plain,
        });
        index += 1;
    }

    tokens
}

pub(super) fn split_leading_whitespace(line: &str) -> (&str, &str) {
    let split_at = line
        .char_indices()
        .find(|(_, ch)| !ch.is_whitespace())
        .map(|(index, _)| index)
        .unwrap_or(line.len());
    line.split_at(split_at)
}

pub(super) fn consume_quoted_string(chars: &[char], start: usize) -> usize {
    let len = chars.len();
    let quote = chars[start];
    let mut index = start + 1;
    while index < len {
        if chars[index] == '\\' && index + 1 < len {
            index += 2;
            continue;
        }
        let current = chars[index];
        index += 1;
        if current == quote {
            break;
        }
    }
    index
}

fn next_non_whitespace(chars: &[char], mut index: usize) -> Option<char> {
    while index < chars.len() {
        if !chars[index].is_whitespace() {
            return Some(chars[index]);
        }
        index += 1;
    }
    None
}

pub(super) fn starts_comment(chars: &[char], index: usize, style: CommentStyle) -> bool {
    match style {
        CommentStyle::Hash => chars[index] == '#',
        CommentStyle::SlashSlash => {
            chars[index] == '/' && index + 1 < chars.len() && chars[index + 1] == '/'
        }
        CommentStyle::HashOrSlashSlash => {
            chars[index] == '#'
                || (chars[index] == '/' && index + 1 < chars.len() && chars[index + 1] == '/')
        }
        CommentStyle::ShellHash => {
            chars[index] == '#' && (index == 0 || chars[index - 1].is_whitespace())
        }
    }
}

pub(super) fn is_word_start(ch: char) -> bool {
    ch.is_alphabetic() || ch == '_'
}

pub(super) fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

pub(super) fn is_plain_keyword(word: &str) -> bool {
    matches!(
        word,
        "def"
            | "class"
            | "if"
            | "elif"
            | "else"
            | "for"
            | "while"
            | "return"
            | "import"
            | "from"
            | "with"
            | "try"
            | "except"
            | "finally"
            | "raise"
            | "pass"
            | "break"
            | "continue"
            | "and"
            | "or"
            | "not"
            | "None"
            | "True"
            | "False"
            | "self"
            | "async"
            | "await"
            | "yield"
            | "lambda"
            | "in"
            | "is"
            | "as"
            | "const"
            | "let"
            | "var"
            | "function"
            | "new"
            | "this"
            | "typeof"
            | "instanceof"
            | "export"
            | "default"
            | "switch"
            | "case"
            | "null"
            | "undefined"
            | "true"
            | "false"
            | "throw"
            | "catch"
            | "extends"
            | "implements"
            | "interface"
            | "readonly"
            | "abstract"
            | "fn"
            | "mut"
            | "pub"
            | "struct"
            | "enum"
            | "impl"
            | "trait"
            | "use"
            | "mod"
            | "match"
            | "crate"
            | "super"
            | "move"
            | "dyn"
            | "unsafe"
            | "extern"
            | "ref"
            | "where"
            | "type"
            | "func"
            | "package"
            | "defer"
            | "chan"
            | "select"
            | "range"
            | "void"
            | "static"
            | "final"
            | "private"
            | "protected"
            | "public"
            | "override"
            | "do"
            | "int"
            | "string"
            | "bool"
            | "float"
    )
}

pub(super) fn is_python_keyword(word: &str) -> bool {
    matches!(
        word,
        "def"
            | "class"
            | "if"
            | "elif"
            | "else"
            | "for"
            | "while"
            | "return"
            | "import"
            | "from"
            | "with"
            | "try"
            | "except"
            | "finally"
            | "raise"
            | "pass"
            | "break"
            | "continue"
            | "and"
            | "or"
            | "not"
            | "None"
            | "True"
            | "False"
            | "self"
            | "async"
            | "await"
            | "yield"
            | "lambda"
            | "in"
            | "is"
            | "as"
            | "global"
            | "nonlocal"
            | "assert"
    )
}

pub(super) fn is_rust_keyword(word: &str) -> bool {
    matches!(
        word,
        "fn" | "let"
            | "mut"
            | "pub"
            | "struct"
            | "enum"
            | "impl"
            | "trait"
            | "use"
            | "mod"
            | "match"
            | "crate"
            | "super"
            | "self"
            | "Self"
            | "move"
            | "dyn"
            | "unsafe"
            | "extern"
            | "ref"
            | "where"
            | "type"
            | "const"
            | "static"
            | "async"
            | "await"
            | "if"
            | "else"
            | "loop"
            | "while"
            | "for"
            | "in"
            | "return"
            | "break"
            | "continue"
            | "true"
            | "false"
            | "None"
            | "Some"
            | "Ok"
            | "Err"
    )
}

pub(super) fn is_shell_keyword(word: &str) -> bool {
    matches!(
        word,
        "if" | "then"
            | "else"
            | "elif"
            | "fi"
            | "for"
            | "while"
            | "do"
            | "done"
            | "case"
            | "esac"
            | "in"
            | "function"
            | "export"
            | "local"
            | "readonly"
            | "declare"
            | "typeset"
            | "source"
            | "alias"
            | "unset"
    )
}
