//! "Commands on connect": lines a network sends right after registering,
//! before joining channels. Typical uses are logging in to a services bot
//! that does not support SASL (Undernet's X, QuakeNet's Q) and setting user
//! modes. Users write them the way they would in a client (`/msg X login ...`);
//! lines without a leading `/` are sent as raw IRC.

use anyhow::{bail, Result};

/// Maximum number of commands per network.
pub const MAX_COMMANDS: usize = 20;
/// Maximum length of one IRC line, without CRLF.
const MAX_LINE: usize = 510;

/// Converts the user's text into raw IRC lines. `$nick` is replaced with the
/// current nickname. Blank lines are ignored.
pub fn to_irc_lines(text: &str, nick: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let line = line.replace("$nick", nick);
        let irc = convert(&line).map_err(|e| anyhow::anyhow!("line {}: {e}", i + 1))?;
        if irc.len() > MAX_LINE {
            bail!("line {}: too long", i + 1);
        }
        out.push(irc);
    }
    if out.len() > MAX_COMMANDS {
        bail!("at most {MAX_COMMANDS} commands are allowed");
    }
    Ok(out)
}

/// Checks the text without a real nickname (used when saving).
pub fn validate(text: &str) -> Result<()> {
    to_irc_lines(text, "nick").map(|_| ())
}

fn convert(line: &str) -> Result<String> {
    if line.contains(['\r', '\n', '\0']) {
        bail!("control characters are not allowed");
    }
    let Some(rest) = line.strip_prefix('/') else {
        return Ok(line.to_string());
    };
    let (cmd, args) = match rest.split_once(char::is_whitespace) {
        Some((c, a)) => (c, a.trim()),
        None => (rest, ""),
    };
    let lower = cmd.to_ascii_lowercase();
    let need_text = |what: &str| -> Result<(String, String)> {
        match args.split_once(char::is_whitespace) {
            Some((t, m)) if !m.trim().is_empty() => Ok((t.to_string(), m.trim().to_string())),
            _ => bail!("/{what} needs a target and a message"),
        }
    };
    Ok(match lower.as_str() {
        "msg" | "privmsg" | "query" => {
            let (t, m) = need_text(&lower)?;
            format!("PRIVMSG {t} :{m}")
        }
        "notice" => {
            let (t, m) = need_text("notice")?;
            format!("NOTICE {t} :{m}")
        }
        "quote" | "raw" => {
            if args.is_empty() {
                bail!("/{lower} needs a command");
            }
            args.to_string()
        }
        "" => bail!("empty command"),
        _ => {
            if !cmd.chars().all(|c| c.is_ascii_alphanumeric()) {
                bail!("invalid command /{cmd}");
            }
            if args.is_empty() {
                cmd.to_ascii_uppercase()
            } else {
                format!("{} {args}", cmd.to_ascii_uppercase())
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_client_style_commands() {
        let text = "/msg x@channels.undernet.org login yo secreto\n\n/mode $nick +x\nAWAY :fuera\n/quote CAP LS\n/notice bob hola que tal";
        assert_eq!(
            to_irc_lines(text, "unixfool").unwrap(),
            vec![
                "PRIVMSG x@channels.undernet.org :login yo secreto",
                "MODE unixfool +x",
                "AWAY :fuera",
                "CAP LS",
                "NOTICE bob :hola que tal",
            ]
        );
    }

    #[test]
    fn rejects_bad_input() {
        assert!(validate("/msg nickserv").is_err());
        assert!(validate("/").is_err());
        assert!(validate("/a$b x").is_err());
        assert!(validate(&"PING x\n".repeat(MAX_COMMANDS + 1)).is_err());
        assert!(validate(&format!("PRIVMSG x :{}", "a".repeat(600))).is_err());
        assert!(validate("").is_ok());
    }
}
