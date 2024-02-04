use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use tantivy::{
    aggregation::{
        agg_req::Aggregations,
        agg_result::{AggregationResult, BucketEntry, BucketResult},
        AggregationCollector,
    },
    collector::{Count, TopDocs},
    query::Query,
    DocAddress, Index,
};

static PAGE_SIZE: usize = 30;

pub struct SearchResult {
    pub count: usize,
    pub docs: Vec<(f32, DocAddress)>,
    pub tags: Option<Vec<BucketEntry>>,
    pub timing: Duration,
}

// TODO: error
pub fn search(index: Arc<Index>, query: &dyn Query, page: usize) -> Option<SearchResult> {
    let now = Instant::now();

    let reader = index.reader().unwrap();
    let searcher = reader.searcher();

    let agg_req: Aggregations = serde_json::from_str(
        r#"{
      "tags": {
        "terms": { "field": "tags", "size": 50 }
      }
    }"#,
    )
    .unwrap();

    let (count, docs, agg_res) = searcher
        .search(
            query,
            &(
                Count,
                TopDocs::with_limit(PAGE_SIZE).and_offset(PAGE_SIZE * page),
                AggregationCollector::from_aggs(agg_req, Default::default()),
            ),
        )
        .unwrap();

    let tags = match agg_res.0.get("tags").unwrap() {
        AggregationResult::BucketResult(bucket_result) => {
            if let BucketResult::Terms { buckets: terms, .. } = bucket_result {
                Some(terms.to_owned())
            } else {
                None
            }
        }
        _ => None,
    };

    let timing = now.elapsed();

    Some(SearchResult {
        count,
        docs,
        tags,
        timing,
    })
}
