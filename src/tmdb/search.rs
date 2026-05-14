#![allow(dead_code)]

use anyhow::Result;

use crate::domain::{MediaKind, TmdbMatch};
use crate::tmdb::model::{extract_year, SearchMovieResponse, SearchTvResponse};
use crate::tmdb::TmdbClient;

impl TmdbClient {
    pub async fn search_movie(&self, query: &str, year: Option<u16>) -> Result<Vec<TmdbMatch>> {
        let year_str = year.map(|y| y.to_string());
        let mut params: Vec<(&str, &str)> = vec![("query", query), ("language", "en-US")];
        if let Some(ref y) = year_str {
            params.push(("year", y));
        }
        let resp: SearchMovieResponse = self.get_json("/search/movie", &params).await?;
        Ok(resp
            .results
            .into_iter()
            .filter_map(|r| {
                let y = extract_year(r.release_date.as_deref())?;
                Some(TmdbMatch {
                    id: r.id,
                    kind: MediaKind::Movie,
                    canonical_title: r.title,
                    year: y,
                    poster_path: r.poster_path,
                })
            })
            .collect())
    }

    pub async fn search_tv(&self, query: &str, year: Option<u16>) -> Result<Vec<TmdbMatch>> {
        let year_str = year.map(|y| y.to_string());
        let mut params: Vec<(&str, &str)> = vec![("query", query), ("language", "en-US")];
        if let Some(ref y) = year_str {
            params.push(("first_air_date_year", y));
        }
        let resp: SearchTvResponse = self.get_json("/search/tv", &params).await?;
        Ok(resp
            .results
            .into_iter()
            .filter_map(|r| {
                let y = extract_year(r.first_air_date.as_deref())?;
                Some(TmdbMatch {
                    id: r.id,
                    kind: MediaKind::Tv,
                    canonical_title: r.name,
                    year: y,
                    poster_path: r.poster_path,
                })
            })
            .collect())
    }

    pub async fn search(
        &self,
        kind: MediaKind,
        query: &str,
        year: Option<u16>,
    ) -> Result<Vec<TmdbMatch>> {
        match kind {
            MediaKind::Movie => self.search_movie(query, year).await,
            MediaKind::Tv => self.search_tv(query, year).await,
        }
    }
}
