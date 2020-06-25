use crate::{error_handling::ErrorHandling, hashed_regex::HashedRegex};
use anyhow::Error;
use http::header::{HeaderName, HeaderValue};
use log::Level;
use regex::{Captures, Regex, Replacer};
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
    /// A list of URL patterns to ignore when checking remote links.
    #[serde(default)]
    pub exclude: Vec<HashedRegex>,
    /// The user-agent used whenever any web requests are made.
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
    /// The number of seconds a cached result is valid for.
    #[serde(default = "default_cache_timeout")]
    pub cache_timeout: u64,
    /// The map of regexes representing sets of web sites and
    /// the list of HTTP headers that must be sent to matching sites.
    #[serde(default)]
    pub http_headers: HashMap<HashedRegex, Vec<HttpHeader>>,
    /// How should non-valid links be handled?
    #[serde(default)]
    pub error_handling: ErrorHandling,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(try_from = "String", into = "String")]
pub struct HttpHeader {
    pub name: HeaderName,
    pub value: String,
}

impl HttpHeader {
    pub(crate) fn interpolate(&self) -> Result<HeaderValue, Error> {
        interpolate_env(&self.value, |var| std::env::var(var).ok())
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
        error_handling: &ErrorHandling,
    ) -> Vec<(HashedRegex, Vec<(HeaderName, HeaderValue)>)> {
        let mut all_headers = Vec::new();

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
                        error_handling
                            .on_header_interpolation_error(header, &e);
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
            exclude: Vec::new(),
            user_agent: default_user_agent(),
            http_headers: HashMap::new(),
            cache_timeout: Config::DEFAULT_CACHE_TIMEOUT.as_secs(),
            error_handling: ErrorHandling::default(),
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

lazy_static::lazy_static! {
    static ref INTERPOLATED_VARIABLE: Regex = Regex::new(r"(?x)
        (?P<escape>\\)?
        \$
        (?P<variable>[\w_][\w_\d]*)
    ").unwrap();
}

fn interpolate_env<F>(value: &str, get_var: F) -> Result<HeaderValue, Error>
where
    F: FnMut(&str) -> Option<String>,
{
    let mut failed_replacements: Vec<String> = Vec::new();

    let interpolated = INTERPOLATED_VARIABLE
        .replace_all(value, replacer(&mut failed_replacements, get_var));

    if failed_replacements.is_empty() {
        interpolated.parse().map_err(Error::from)
    } else {
        Err(Error::from(InterpolationError {
            variable_names: failed_replacements,
            original_string: value.to_string(),
        }))
    }
}

/// Gets a `Replacer` which will try to replace a variable with the result
/// from the `get_var()` function, recording any errors that happen.
fn replacer<'a, V>(
    failed_replacements: &'a mut Vec<String>,
    mut get_var: V,
) -> impl Replacer + 'a
where
    V: FnMut(&str) -> Option<String> + 'a,
{
    move |caps: &Captures<'_>| {
        if caps.name("escape").is_none() {
            let variable = &caps["variable"];

            match get_var(variable) {
                Some(value) => return value,
                None => {
                    failed_replacements.push(variable.to_string());
                },
            }
        }

        // the dollar sign was escaped (e.g. "\$foo") or we couldn't get
        // the environment variable
        caps[0].to_string()
    }
}

#[derive(Debug, Clone)]
struct InterpolationError {
    pub variable_names: Vec<String>,
    pub original_string: String,
}

impl std::error::Error for InterpolationError {}

impl Display for InterpolationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.variable_names.len() == 1 {
            write!(
                f,
                "Unable to interpolate `${}` into \"{}\"",
                &self.variable_names[0], self.original_string
            )
        } else {
            let formatted_names: Vec<_> = self
                .variable_names
                .iter()
                .map(|v| format!("`${}`", v))
                .collect();

            write!(
                f,
                "Unable to interpolate `${}` into \"{}\"",
                formatted_names.join(", "),
                self.original_string
            )
        }
    }
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

[http-headers]
https = ["accept: html/text", "authorization: Basic $TOKEN"]
"#;

    #[test]
    fn deserialize_a_config() {
        std::env::set_var("TOKEN", "QWxhZGRpbjpPcGVuU2VzYW1l");

        let should_be = Config {
            follow_web_links: true,
            traverse_parent_directories: true,
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
            error_handling: ErrorHandling::default(),
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

    #[test]
    fn interplate_a_single_variable() {
        let text = "Hello, $name";

        let got = interpolate_env(text, |name| {
            if name == "name" {
                Some(String::from("World!"))
            } else {
                None
            }
        })
        .unwrap();

        assert_eq!(got, "Hello, World!");
    }

    #[test]
    fn you_can_skip_interpolation_by_escaping_the_dollar_sign() {
        let text = r"Hello, \$name";

        let got = interpolate_env(text, |_| unreachable!()).unwrap();

        assert_eq!(got, text);
    }

    #[test]
    fn not_having_the_requested_variable_is_an_error() {
        let text = r"Hello, $name";
        let never_works = |_name: &str| None;

        let got = interpolate_env(text, never_works).unwrap_err();

        let inner = got.downcast::<InterpolationError>().unwrap();
        assert_eq!(inner.variable_names, vec![String::from("name")]);
    }
}
