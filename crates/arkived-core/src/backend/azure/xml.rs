//! Shared XML deserialization helpers for Azure REST responses.

use crate::Error;
use quick_xml::de::from_str;
use serde::de::DeserializeOwned;

/// Parse a response body as XML.
pub(crate) fn parse_xml<T: DeserializeOwned>(body: &str) -> crate::Result<T> {
    from_str(body).map_err(|e| Error::Backend(format!("parse xml: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct Greeting {
        #[serde(rename = "$text")]
        text: String,
    }

    #[test]
    fn round_trips_simple_xml() {
        let doc = r#"<Greeting>hello</Greeting>"#;
        let g: Greeting = parse_xml(doc).unwrap();
        assert_eq!(g.text, "hello");
    }

    #[test]
    fn malformed_becomes_backend_error() {
        let err: crate::Result<Greeting> = parse_xml("<not xml>");
        assert!(matches!(err, Err(Error::Backend(_))));
    }
}
