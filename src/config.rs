/// The configuration options available with this backend.
#[derive(Debug, Copy, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Config {
    /// If a link on the internet is encountered, should we still try to check
    /// if it's valid? Defaults to `false` because this has a big performance
    /// impact.
    pub follow_web_links: bool,
}
