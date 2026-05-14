#![allow(dead_code)]

use anyhow::Result;

use crate::domain::MediaKind;
use crate::tmdb::model::{Image, ImagesResponse, TvDetails};
use crate::tmdb::TmdbClient;

impl TmdbClient {
    async fn movie_images(&self, id: u64) -> Result<Vec<Image>> {
        let resp: ImagesResponse = self.get_json(&format!("/movie/{id}/images"), &[]).await?;
        Ok(resp.posters)
    }

    async fn tv_images(&self, id: u64) -> Result<Vec<Image>> {
        let resp: ImagesResponse = self.get_json(&format!("/tv/{id}/images"), &[]).await?;
        Ok(resp.posters)
    }

    pub async fn tv_season_images(&self, id: u64, season: u8) -> Result<Vec<Image>> {
        let resp: ImagesResponse = self
            .get_json(&format!("/tv/{id}/season/{season}/images"), &[])
            .await?;
        Ok(resp.posters)
    }

    pub async fn tv_details(&self, id: u64) -> Result<TvDetails> {
        self.get_json(&format!("/tv/{id}"), &[("language", "en-US")])
            .await
    }

    /// Pull the best poster file_path for a title. Tries `iso_639_1 == "en"`
    /// first, sorted by vote_average; then falls back to no-language posters,
    /// then to anything.
    pub async fn pick_best_poster_path(&self, kind: MediaKind, id: u64) -> Result<Option<String>> {
        let imgs = match kind {
            MediaKind::Movie => self.movie_images(id).await?,
            MediaKind::Tv => self.tv_images(id).await?,
        };
        Ok(pick_best_poster(&imgs).map(|i| i.file_path.clone()))
    }
}

pub fn pick_best_poster(imgs: &[Image]) -> Option<&Image> {
    let mut english: Vec<&Image> = imgs
        .iter()
        .filter(|i| i.iso_639_1.as_deref() == Some("en"))
        .collect();
    english.sort_by(|a, b| {
        b.vote_average
            .partial_cmp(&a.vote_average)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if let Some(first) = english.first() {
        return Some(first);
    }

    let mut none: Vec<&Image> = imgs.iter().filter(|i| i.iso_639_1.is_none()).collect();
    none.sort_by(|a, b| {
        b.vote_average
            .partial_cmp(&a.vote_average)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if let Some(first) = none.first() {
        return Some(first);
    }

    let mut all: Vec<&Image> = imgs.iter().collect();
    all.sort_by(|a, b| {
        b.vote_average
            .partial_cmp(&a.vote_average)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    all.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(lang: Option<&str>, vote: f64) -> Image {
        Image {
            file_path: format!("/{:?}_{}.jpg", lang, vote),
            vote_average: vote,
            vote_count: 1,
            iso_639_1: lang.map(|s| s.to_string()),
            width: None,
            height: None,
        }
    }

    #[test]
    fn prefers_english_high_vote() {
        let imgs = vec![
            img(Some("en"), 5.0),
            img(Some("en"), 9.0),
            img(Some("ru"), 9.5),
        ];
        let best = pick_best_poster(&imgs).unwrap();
        assert_eq!(best.iso_639_1.as_deref(), Some("en"));
        assert_eq!(best.vote_average, 9.0);
    }

    #[test]
    fn falls_back_to_no_lang() {
        let imgs = vec![img(None, 6.0), img(Some("ru"), 9.5)];
        let best = pick_best_poster(&imgs).unwrap();
        assert!(best.iso_639_1.is_none());
    }

    #[test]
    fn falls_back_to_any() {
        let imgs = vec![img(Some("ru"), 7.0), img(Some("ja"), 8.0)];
        let best = pick_best_poster(&imgs).unwrap();
        assert_eq!(best.vote_average, 8.0);
    }
}
