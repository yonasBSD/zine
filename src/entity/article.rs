use std::{fs, path::Path};

use anyhow::Result;
use once_cell::sync::Lazy;
use pulldown_cmark::{html, Options, Parser};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tera::Context;

use crate::Render;

use super::{EndMatter, Entity};

#[derive(Serialize, Deserialize)]
pub struct Article {
    pub file: String,
    // The slug after this artcile rendered.
    // Default to file name if no slug specified.
    pub slug: Option<String>,
    pub title: String,
    pub author: Option<String>,
    pub cover: Option<String>,
    #[serde(default)]
    pub html: String,
    // TODO: deserialize to OffsetDateTime
    pub pub_date: String,
    // The optional end matter of the article.
    pub end_matter: Option<EndMatter>,
    // Wheter the article is an featured article.
    // Featured article will display in home page.
    #[serde(default)]
    pub featured: bool,
    #[serde(default)]
    pub publish: bool,
}

impl std::fmt::Debug for Article {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Article")
            .field("file", &self.file)
            .field("slug", &self.slug)
            .field("title", &self.title)
            .field("author", &self.author)
            .field("cover", &self.cover)
            .field("pub_date", &self.pub_date)
            .field("publish", &self.publish)
            .finish()
    }
}

impl Article {
    pub fn slug(&self) -> String {
        self.slug
            .as_ref()
            .cloned()
            .unwrap_or_else(|| self.file.replace(".md", ""))
    }
}

impl Entity for Article {
    fn parse(&mut self, source: &Path) -> Result<()> {
        let markdown = fs::read_to_string(&source.join(&self.file))?;
        let (article, end_matter) = split_article_content(&markdown)?;
        let markdown_parser = Parser::new_ext(article, Options::all());
        html::push_html(&mut self.html, markdown_parser);

        self.end_matter = end_matter;
        Ok(())
    }

    fn render(&self, mut context: Context, dest: &Path) -> Result<()> {
        context.insert("article", &self);
        Render::render("article.jinja", &context, dest)?;
        Ok(())
    }
}

static END_MATTER_REGEX: Lazy<Regex> = Lazy::new(|| {
    // The regex is an variant of zola's fronmatter regex.
    Regex::new(
        r"^[[:space:]]*(?:$|(?:\r?\n((?s).*(?-s))))[[:space:]]*\+\+\+(\r?\n(?s).*?(?-s))\+\+\+[[:space:]]*$",
    )
    .unwrap()
});

// Splite article content and optional end matter from article markdown.
fn split_article_content(markdown: &str) -> Result<(&str, Option<EndMatter>)> {
    if let Some(caps) = END_MATTER_REGEX.captures(markdown) {
        // caps[0] is the full match
        // caps[1] => article
        // caps[2] => end matter
        let article = caps.get(1).expect("").as_str().trim();
        let end_matter = caps.get(2).expect("").as_str().trim();
        match toml::from_str::<EndMatter>(end_matter) {
            Ok(end_matter) => {
                return Ok((article, Some(end_matter)));
            }
            // Parse failed if the end matter has invalid toml syntax.
            Err(error) => println!("Parse end matter error: {}", error),
        }
    }

    Ok((markdown, None))
}

#[cfg(test)]
mod tests {
    use test_case::test_case;

    use super::split_article_content;

    #[test_case(r#"
    Hello
    "#; "No end matter")]
    #[test_case(r#"
    Hello
    +++
    "#; "Invalid end matter")]
    #[test_case(r#"
    Hello
    +++
    +++
    "#; "Empty end matter")]
    fn test_parse_end_matter_none(input: &str) {
        let r = split_article_content(input).unwrap();
        assert!(r.1.is_none());
    }

    #[test_case(r#"
    Hello
    +++
    [[abc]]
    +++
    "#; "Invalid end matter1")]
    #[test_case(r#"
    Hello
    +++
    [[comment]]
    xxx = "yyy"
    +++
    "#; "Invalid end matter2")]
    #[test_case(r#"
    Hello
    +++
    [[comment]]
    author = 123
    content = 123
    +++
    "#; "Invalid end matter3")]
    fn test_parse_end_matter_invalid(input: &str) {
        let (_, end_matter) = split_article_content(input).unwrap();
        assert!(end_matter.is_none());
    }

    #[test_case(r#"
    Hello
    +++
    [[comment]]
    author = "Alice"
    content = "Hi"
    +++
    "#; "Normal end matter")]
    fn test_parse_end_matter_normal(input: &str) {
        let (_, end_matter) = split_article_content(input).unwrap();
        let end_matter = end_matter.unwrap();
        assert_eq!(1, end_matter.comments.len());
        let comment = end_matter.comments.iter().next().unwrap();
        assert_eq!("Alice", comment.author);
        assert_eq!(None, comment.link);
        assert_eq!("Hi", comment.content);
    }

    #[test_case(r#"
    Hello
    +++
    [[comment]]
    author = "Alice"
    link = "https://github.com"
    content = "Hi"
    +++
    "#; "Single comment in end matter")]
    fn test_parse_end_matter_full(input: &str) {
        let (_, end_matter) = split_article_content(input).unwrap();
        let end_matter = end_matter.unwrap();
        assert_eq!(1, end_matter.comments.len());
        let comment = end_matter.comments.iter().next().unwrap();
        assert_eq!("Alice", comment.author);
        assert_eq!(Some("https://github.com".into()), comment.link);
        assert_eq!("Hi", comment.content);
    }

    #[test_case(r#"
    Hello
    +++
    [[comment]]
    author = "Alice"
    content = "Hi"

    [[comment]]
    author = "Bob"
    link = "https://github.com"
    content = "Hey"
    +++
    "#; "Multipe comments in end matter")]
    fn test_parse_end_matter_multiple(input: &str) {
        let (_, end_matter) = split_article_content(input).unwrap();
        let end_matter = end_matter.unwrap();
        let mut iter = end_matter.comments.iter();
        assert_eq!(2, iter.len());

        let comment = iter.next().unwrap();
        assert_eq!("Alice", comment.author);
        assert_eq!(None, comment.link);
        assert_eq!("Hi", comment.content);

        let comment = iter.next().unwrap();
        assert_eq!("Bob", comment.author);
        assert_eq!(Some("https://github.com".into()), comment.link);
        assert_eq!("Hey", comment.content);
    }
}
