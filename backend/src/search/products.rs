use super::IndexDefinition;
use crate::embeddings::{DIMENSIONS, MODEL};
use meilisearch_sdk::settings::{Embedder, EmbedderSource, Settings};

pub(super) const INDEX: IndexDefinition = IndexDefinition {
    uid_prefix: "tosca_market_products_v5_amazon",
    entity_type: "product",
    settings,
    displayed: &["id", "name", "lowest_price"],
    enqueue: "INSERT INTO search_sync_jobs(entity_type,entity_id)
        SELECT 'product',id FROM products WHERE TRUE
        ON DUPLICATE KEY UPDATE revision=revision+1,updated_at=CURRENT_TIMESTAMP(6)",
    select_jobs: "SELECT j.entity_id,j.revision,
        CASE WHEN p.status='active' AND NOT s.suspended THEN JSON_OBJECT(
            'id',p.id,'name',p.name,'image',p.image,'shop_id',s.id,'shop',s.name,
            'category',c.name,'brand',b.name,'description',p.description,
            'lowest_price',(SELECT MIN(v.price) FROM variants v WHERE v.product_id=p.id AND v.enabled)
        ) END AS document
        FROM search_sync_jobs j
        LEFT JOIN products p ON p.id=j.entity_id
        LEFT JOIN shops s ON s.id=p.shop_id
        LEFT JOIN taxonomy c ON c.id=p.category_id
        LEFT JOIN taxonomy b ON b.id=p.brand_id
        WHERE j.entity_type='product' ORDER BY j.updated_at,j.entity_id LIMIT ?",
};

fn settings() -> Settings {
    legacy_settings()
        .with_searchable_attributes(["name", "brand", "category"])
        // .with_synonyms(std::collections::HashMap::from([("kopi", ["coffee"])]))
        .with_embedders(std::collections::HashMap::from([(
            MODEL,
            Embedder {
                source: EmbedderSource::UserProvided,
                dimensions: Some(DIMENSIONS),
                ..Default::default()
            },
        )]))
}

pub(super) fn legacy_settings() -> Settings {
    Settings::new()
        .with_searchable_attributes(["name"])
        .with_filterable_attributes(["category", "shop_id"])
        .with_sortable_attributes(["lowest_price", "name"])
}
