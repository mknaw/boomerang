use std::sync::OnceLock;

use opentelemetry::{
    KeyValue, global,
    metrics::{Counter, Histogram},
};

use crate::ai::types::Usage;

static TOKEN_COUNTER: OnceLock<Counter<u64>> = OnceLock::new();
static TOKEN_HISTOGRAM: OnceLock<Histogram<u64>> = OnceLock::new();

fn get_token_counter() -> &'static Counter<u64> {
    TOKEN_COUNTER.get_or_init(|| {
        global::meter("boomerang")
            .u64_counter("llm.tokens.total")
            .with_description("Total number of LLM tokens consumed")
            .with_unit("{token}")
            .build()
    })
}

fn get_token_histogram() -> &'static Histogram<u64> {
    TOKEN_HISTOGRAM.get_or_init(|| {
        global::meter("boomerang")
            .u64_histogram("llm.tokens.distribution")
            .with_description("Distribution of LLM token counts per request")
            .with_unit("{token}")
            .build()
    })
}

pub fn record_token_usage(provider: &str, model: &str, usage: &Usage) {
    let counter = get_token_counter();
    let histogram = get_token_histogram();

    let provider_attr = KeyValue::new("provider", provider.to_string());
    let model_attr = KeyValue::new("model", model.to_string());

    if usage.prompt_tokens > 0 {
        let prompt_attrs = vec![
            provider_attr.clone(),
            model_attr.clone(),
            KeyValue::new("token_type", "prompt"),
        ];
        counter.add(usage.prompt_tokens as u64, &prompt_attrs);
        histogram.record(usage.prompt_tokens as u64, &prompt_attrs);
    }

    if usage.completion_tokens > 0 {
        let completion_attrs = vec![
            provider_attr.clone(),
            model_attr.clone(),
            KeyValue::new("token_type", "completion"),
        ];
        counter.add(usage.completion_tokens as u64, &completion_attrs);
        histogram.record(usage.completion_tokens as u64, &completion_attrs);
    }

    if usage.total_tokens > 0 {
        let total_attrs = vec![
            provider_attr,
            model_attr,
            KeyValue::new("token_type", "total"),
        ];
        counter.add(usage.total_tokens as u64, &total_attrs);
        histogram.record(usage.total_tokens as u64, &total_attrs);
    }
}
