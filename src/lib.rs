extern crate mdbook;
#[macro_use]
extern crate failure;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
extern crate pulldown_cmark;

use failure::Error;
use mdbook::renderer::RenderContext;

pub fn check_links(ctx: &RenderContext) -> Result<(), Error> {
    unimplemented!()
}