use std::sync::Arc;

use tantivy::{
    aggregation::{
        agg_req::Aggregations,
        agg_result::{AggregationResult, AggregationResults, BucketEntry, BucketResult},
        AggregationCollector,
    },
    collector::{MultiCollector, TopDocs},
    query::Query,
    DocAddress, Index,
};

pub struct SearchResult {
    pub docs: Vec<(f32, DocAddress)>,
    pub tags: Option<Vec<BucketEntry>>,
}

// TODO: error
pub fn search(index: Arc<Index>, query: &dyn Query) -> Option<SearchResult> {
    let reader = index.reader().unwrap();
    let searcher = reader.searcher();

    let agg_req: Aggregations = serde_json::from_str(
        r#"{
      "tags": {
        "terms": { "field": "tags" }
      }
    }"#,
    )
    .unwrap();

    let mut collector = MultiCollector::new();
    let agg_handle =
        collector.add_collector(AggregationCollector::from_aggs(agg_req, Default::default()));
    let top_docs_handle = collector.add_collector(TopDocs::with_limit(30));

    let mut search_result = searcher.search(query, &collector).unwrap();

    let top_docs = top_docs_handle.extract(&mut search_result);

    let agg_res: AggregationResults = agg_handle.extract(&mut search_result);
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

    Some(SearchResult {
        docs: top_docs,
        tags,
    })
}
