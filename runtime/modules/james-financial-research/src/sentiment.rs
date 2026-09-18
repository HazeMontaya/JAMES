//! News Sentiment Analysis Engine

use james_adapter_kronos::Symbol;
use crate::error::Result;
use crate::config::NewsSource;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

/// News sentiment engine
pub struct NewsSentimentEngine {
    sources: Vec<NewsSource>,
    api_keys: std::collections::HashMap<NewsSource, String>,
}

impl NewsSentimentEngine {
    pub fn new(sources: Vec<NewsSource>) -> Self {
        Self {
            sources,
            api_keys: std::collections::HashMap::new(),
        }
    }

    pub fn set_api_key(&mut self, source: NewsSource, key: String) {
        self.api_keys.insert(source, key);
    }

    /// Fetch and analyze sentiment for a symbol
    pub async fn analyze_sentiment(&self, symbol: &Symbol, lookback_days: u32) -> Result<SentimentAnalysis> {
        let mut all_articles = Vec::new();
        
        for source in &self.sources {
            if let Ok(articles) = self.fetch_from_source(source, symbol, lookback_days).await {
                all_articles.extend(articles);
            }
        }

        if all_articles.is_empty() {
            return Ok(SentimentAnalysis::neutral());
        }

        let sentiment = self.analyze_articles(&all_articles);
        Ok(sentiment)
    }

    async fn fetch_from_source(&self, source: &NewsSource, symbol: &Symbol, lookback_days: u32) -> Result<Vec<NewsArticle>> {
        // Placeholder - would integrate with actual news APIs
        Ok(Vec::new())
    }

    fn analyze_articles(&self, articles: &[NewsArticle]) -> SentimentAnalysis {
        let mut positive = 0;
        let mut negative = 0;
        let mut neutral = 0;

        for article in articles {
            match article.sentiment {
                ArticleSentiment::Positive => positive += 1,
                ArticleSentiment::Negative => negative += 1,
                ArticleSentiment::Neutral => neutral += 1,
            }
        }

        let total = articles.len() as f64;
        let score = (positive as f64 - negative as f64) / total.max(1.0);

        SentimentAnalysis {
            score: Decimal::from_f64_retain(score).unwrap_or_default(),
            positive_count: positive,
            negative_count: negative,
            neutral_count: neutral,
            article_count: articles.len(),
            confidence: (positive + negative) as f64 / total.max(1.0),
        }
    }
}

/// News article structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsArticle {
    pub title: String,
    pub content: String,
    pub source: String,
    pub url: String,
    pub published_at: DateTime<Utc>,
    pub sentiment: ArticleSentiment,
    pub relevance_score: f64,
}

/// Article sentiment classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArticleSentiment {
    Positive,
    Negative,
    Neutral,
}

/// Sentiment analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentAnalysis {
    pub score: Decimal,        // -1.0 to 1.0
    pub positive_count: usize,
    pub negative_count: usize,
    pub neutral_count: usize,
    pub article_count: usize,
    pub confidence: f64,
}

impl SentimentAnalysis {
    pub fn neutral() -> Self {
        Self {
            score: Decimal::ZERO,
            positive_count: 0,
            negative_count: 0,
            neutral_count: 0,
            article_count: 0,
            confidence: 0.0,
        }
    }
}