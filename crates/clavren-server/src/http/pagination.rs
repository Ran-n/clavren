// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/03 04:54:13.854795
//+ Revised: 	2026/11/03 04:54:13.854795
// -----------------------------------------------------------------------------

//! Cursor pagination for every admin list: `?limit=50&after=<cursor>` answers
//! `{ "data": [...], "next_cursor": "..." | null }`. The cursor is opaque to clients; inside, it
//! holds the sort key and ID of the last row, so a page never skips or repeats a row when rows are
//! added or removed between requests.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::error::ApiError;

pub const DEFAULT_LIMIT: u32 = 50;
pub const MAX_LIMIT: u32 = 100;

#[derive(Debug, Default, Deserialize)]
pub struct PageParams {
    pub limit: Option<u32>,
    pub after: Option<String>,
}

impl PageParams {
    pub fn limit(&self) -> Result<u32, ApiError> {
        match self.limit.unwrap_or(DEFAULT_LIMIT) {
            limit @ 1..=MAX_LIMIT => Ok(limit),
            _ => Err(ApiError::invalid_request(format!("limit must be between 1 and {MAX_LIMIT}"))),
        }
    }

    pub fn cursor(&self) -> Result<Option<Cursor>, ApiError> {
        self.after.as_deref().filter(|after| !after.is_empty()).map(Cursor::decode).transpose()
    }
}

/// Where the previous page ended: the last row's sort key (when the order isn't by ID alone) and
/// ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    #[serde(rename = "k", skip_serializing_if = "Option::is_none", default)]
    pub key: Option<String>,
    #[serde(rename = "i")]
    pub id: Uuid,
}

impl Cursor {
    pub fn encode(&self) -> String {
        // serialising two plain fields can't fail
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(self).unwrap_or_default())
    }

    pub fn decode(text: &str) -> Result<Self, ApiError> {
        URL_SAFE_NO_PAD
            .decode(text)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .ok_or_else(|| ApiError::invalid_request("the cursor is not one this server issued"))
    }
}

#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub data: Vec<T>,
    pub next_cursor: Option<String>,
}

impl<T> Page<T> {
    /// `rows` holds up to `limit + 1` rows: the extra one only says another page exists.
    pub fn from_rows(mut rows: Vec<T>, limit: u32, cursor_of: impl Fn(&T) -> Cursor) -> Self {
        let has_more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_cursor = rows.last().filter(|_| has_more).map(|last| cursor_of(last).encode());
        Self { data: rows, next_cursor }
    }

    pub fn map<U>(self, convert: impl FnMut(T) -> U) -> Page<U> {
        Page { data: self.data.into_iter().map(convert).collect(), next_cursor: self.next_cursor }
    }
}

/// The row count to ask the database for: one more than the page, to learn whether more exist.
pub fn fetch_count(limit: u32) -> i64 {
    i64::from(limit) + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(limit: Option<u32>, after: Option<&str>) -> PageParams {
        PageParams { limit, after: after.map(String::from) }
    }

    #[test]
    fn the_limit_defaults_and_is_bounded() {
        assert_eq!(params(None, None).limit().unwrap(), DEFAULT_LIMIT);
        assert_eq!(params(Some(100), None).limit().unwrap(), 100);
        assert!(params(Some(0), None).limit().is_err());
        assert!(params(Some(101), None).limit().is_err());
    }

    #[test]
    fn a_cursor_round_trips() {
        let cursor = Cursor { key: Some("ana lópez".into()), id: Uuid::now_v7() };
        assert_eq!(Cursor::decode(&cursor.encode()).unwrap(), cursor);
        let by_id = Cursor { key: None, id: Uuid::now_v7() };
        assert_eq!(params(None, Some(&by_id.encode())).cursor().unwrap(), Some(by_id));
        assert_eq!(params(None, Some("")).cursor().unwrap(), None);
    }

    #[test]
    fn a_forged_cursor_is_invalid() {
        for text in ["not base64!", "e30", &URL_SAFE_NO_PAD.encode(b"{\"i\":\"nope\"}")] {
            assert_eq!(Cursor::decode(text).unwrap_err().code(), "invalid_request", "{text}");
        }
    }

    #[test]
    fn a_page_says_whether_more_rows_exist() {
        let ids: Vec<Uuid> = (0..3).map(|_| Uuid::now_v7()).collect();
        let cursor_of = |id: &Uuid| Cursor { key: None, id: *id };
        let full = Page::from_rows(ids.clone(), 2, cursor_of);
        assert_eq!(full.data, ids[..2]);
        assert_eq!(Cursor::decode(full.next_cursor.as_deref().unwrap()).unwrap().id, ids[1]);
        let last = Page::from_rows(ids[..2].to_vec(), 2, cursor_of);
        assert_eq!((last.data.len(), last.next_cursor), (2, None));
        assert_eq!(fetch_count(50), 51);
    }
}
