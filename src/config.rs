use crate::hashed_regex::HashedRegex;
use anyhow::Error;
use http::header::{HeaderName, HeaderValue};
use log::Level;
use reqwest::Client;
use serde_derive::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    convert::TryFrom,
    fmt::{self, Display, Formatter},
    str::FromStr,
    time::Duration,
};

/// The configuration options available with this backend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "kebab-case")]
pub struct Config {
    /// If a link on the internet is encountered, should we still try to check
    /// if it's valid? Defaults to `false` because this has a big performance
    /// impact.
    pub follow_web_links: bool,
    /// Are we allowed to link to files outside of the book's source directory?
    pub traverse_parent_directories: bool,

    /// Should treat symlinks as terminal points?
    pub follow_symlinks: bool,

    /// A list of URL patterns to ignore when checking remote links.
    #[serde(default)]
    pub exclude: Vec<HashedRegex>,
    /// The user-agent used whenever any web requests are made.
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    /// The number of seconds a cached result is valid for.
    #[serde(default = "default_cache_timeout")]
    pub cache_timeout: u64,
    /// The policy to use when warnings are encountered.
    #[serde(default)]
    pub warning_policy: WarningPolicy,
    /// The map of regexes representing sets of web sites and
    /// the list of HTTP headers that must be sent to matching sites.
    #[serde(default)]
    pub http_headers: HashMap<HashedRegex, Vec<HttpHeader>>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(try_from = "String", into = "String")]
pub struct HttpHeader {
    pub name: HeaderName,
    pub value: String,
}

impl HttpHeader {
    pub(crate) fn interpolate(&self) -> Result<HeaderValue, Error> {
        interpolate_env(&self.value)
    }
}

impl Display for HttpHeader {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.value)
    }
}

impl Config {
    /// The default cache timeout (around 12 hours).
    pub const DEFAULT_CACHE_TIMEOUT: Duration =
        Duration::from_secs(60 * 60 * 12);
    /// The default user-agent.
    pub const DEFAULT_USER_AGENT: &'static str =
        concat!(env!("CARGO_PKG_NAME"), "-", env!("CARGO_PKG_VERSION"));

    /// Checks [`Config::exclude`] to see if the provided link should be
    /// skipped.
    pub fn should_skip(&self, link: &str) -> bool {
        self.exclude.iter().any(|pat| pat.find(link).is_some())
    }

    pub(crate) fn client(&self) -> Client {
        let mut headers = http::HeaderMap::new();
        headers
            .insert(http::header::USER_AGENT, self.user_agent.parse().unwrap());
        Client::builder().default_headers(headers).build().unwrap()
    }

    pub(crate) fn interpolate_headers(
        &self,
        warning_policy: WarningPolicy,
    ) -> Vec<(HashedRegex, Vec<(HeaderName, HeaderValue)>)> {
        let mut all_headers = Vec::new();
        let log_level = warning_policy.to_log_level();

        for (pattern, headers) in &self.http_headers {
            let mut interpolated = Vec::new();

            for header in headers {
                match header.interpolate() {
                    Ok(value) => {
                        interpolated.push((header.name.clone(), value))
                    },
                    Err(e) => {
                        // We don't want failed interpolation (i.e. due to a
                        // missing env variable) to abort the whole
                        // linkchecking, so emit a warning and keep going.
                        //
                        // If it was important, the user would notice a "broken"
                        // link and read back through the logs.
                        log::log!(
                            log_level,
                            "Unable to interpolate \"{}\" because {}",
                            header,
                            e
                        );
                    },
                }
            }

            all_headers.push((pattern.clone(), interpolated));
        }

        all_headers
    }
}

impl Default for Config {
    fn default() -> Config {
        Config {
            follow_web_links: false,
            traverse_parent_directories: false,
            follow_symlinks: true,
            exclude: Vec::new(),
            user_agent: default_user_agent(),
            http_headers: HashMap::new(),
            warning_policy: WarningPolicy::Warn,
            cache_timeout: Config::DEFAULT_CACHE_TIMEOUT.as_secs(),
        }
    }
}

impl FromStr for HttpHeader {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.find(": ") {
            Some(idx) => {
                let name = s[..idx].parse()?;
                let value = s[idx + 2..].to_string();
                Ok(HttpHeader {
                    name,
                    value,
                })
            },

            None => Err(Error::msg(format!(
                "The `{}` HTTP header must be in the form `key: value` but it isn't",
                s
            ))),
        }
    }
}

impl TryFrom<&'_ str> for HttpHeader {
    type Error = Error;

    fn try_from(s: &'_ str) -> Result<Self, Error> { HttpHeader::from_str(s) }
}

impl TryFrom<String> for HttpHeader {
    type Error = Error;

    fn try_from(s: String) -> Result<Self, Error> {
        HttpHeader::try_from(s.as_str())
    }
}

impl Into<String> for HttpHeader {
    fn into(self) -> String {
        let HttpHeader { name, value, .. } = self;
        format!("{}: {}", name, value)
    }
}

fn default_cache_timeout() -> u64 { Config::DEFAULT_CACHE_TIMEOUT.as_secs() }
fn default_user_agent() -> String { Config::DEFAULT_USER_AGENT.to_string() }

fn interpolate_env(value: &str) -> Result<HeaderValue, Error> {
    use std::{iter::Peekable, str::CharIndices};

    fn is_ident(ch: char) -> bool { ch.is_ascii_alphanumeric() || ch == '_' }

    fn ident_end(start: usize, iter: &mut Peekable<CharIndices>) -> usize {
        let mut end = start;
        while let Some(&(i, ch)) = iter.peek() {
            if !is_ident(ch) {
                return i;
            }
            end = i + ch.len_utf8();
            iter.next();
        }

        end
    }

    let mut res = String::with_capacity(value.len());
    let mut backslash = false;
    let mut iter = value.char_indices().peekable();

    while let Some((i, ch)) = iter.next() {
        if backslash {
            match ch {
                '$' | '\\' => res.push(ch),
                _ => {
                    res.push('\\');
                    res.push(ch);
                },
            }

            backslash = false;
        } else {
            match ch {
                '\\' => backslash = true,
                '$' => {
                    iter.next();
                    let start = i + 1;
                    let end = ident_end(start, &mut iter);
                    let name = &value[start..end];

                    match std::env::var(name) {
                        Ok(env) => res.push_str(&env),
                        Err(e) => {
                            return Err(Error::msg(format!(
                                "Failed to retrieve `{}` env var: {}",
                                name, e
                            )))
                        },
                    }
                },

                _ => res.push(ch),
            }
        }
    }

    // trailing backslash
    if backslash {
        res.push('\\');
    }

    Ok(res.parse()?)
}

/// How should warnings be treated?
#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WarningPolicy {
    /// Silently ignore them.
    Ignore,
    /// Warn the user, but don't fail the linkcheck.
    Warn,
    /// Treat warnings as errors.
    Error,
}

impl WarningPolicy {
    pub(crate) fn to_log_level(self) -> Level {
        match self {
            WarningPolicy::Error => Level::Error,
            WarningPolicy::Warn => Level::Warn,
            WarningPolicy::Ignore => Level::Debug,
        }
    }
}

impl Default for WarningPolicy {
    fn default() -> WarningPolicy { WarningPolicy::Warn }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{convert::TryInto, iter::FromIterator};
    use toml;

    const CONFIG: &str = r#"follow-web-links = true
traverse-parent-directories = true
exclude = ["google\\.com"]
user-agent = "Internet Explorer"
cache-timeout = 3600
warning-policy = "error"

[http-headers]
https = ["accept: html/text", "authorization: Basic $TOKEN"]
"#;

    #[test]
    fn deserialize_a_config() {
        std::env::set_var("TOKEN", "QWxhZGRpbjpPcGVuU2VzYW1l");

        let should_be = Config {
            follow_web_links: true,
            warning_policy: WarningPolicy::Error,
            traverse_parent_directories: true,
            follow_symlinks: true,
            exclude: vec![HashedRegex::new(r"google\.com").unwrap()],
            user_agent: String::from("Internet Explorer"),
            http_headers: HashMap::from_iter(vec![(
                HashedRegex::new("https").unwrap(),
                vec![
                    "Accept: html/text".try_into().unwrap(),
                    "Authorization: Basic $TOKEN".try_into().unwrap(),
                ],
            )]),
            cache_timeout: 3600,
        };

        let got: Config = toml::from_str(CONFIG).unwrap();

        assert_eq!(got, should_be);
    }

    #[test]
    fn round_trip_config() {
        // A check that a value of an env var is not leaked in the
        // deserialization
        std::env::set_var("TOKEN", "QWxhZGRpbjpPcGVuU2VzYW1l");

        let deserialized: Config = toml::from_str(CONFIG).unwrap();
        let reserialized = toml::to_string(&deserialized).unwrap();

        assert_eq!(reserialized, CONFIG);
    }

    #[test]
    fn interpolation() {
        std::env::set_var("SUPER_SECRET_TOKEN", "abcdefg123456");
        let header = HttpHeader {
            name: "Authorization".parse().unwrap(),
            value: "Basic $SUPER_SECRET_TOKEN".into(),
        };
        let should_be: HeaderValue = "Basic abcdefg123456".parse().unwrap();

        let got = header.interpolate().unwrap();

        assert_eq!(got, should_be);
    }
}
