use super::IndexDefinition;
use meilisearch_sdk::settings::Settings;

pub(super) const INDEX: IndexDefinition = IndexDefinition {
    uid_prefix: "tosca_market_shops_v5",
    entity_type: "shop",
    settings,
    displayed: &["id", "name"],
    enqueue: "INSERT INTO search_sync_jobs(entity_type,entity_id)
        SELECT 'shop',id FROM shops WHERE TRUE
        ON DUPLICATE KEY UPDATE revision=revision+1,updated_at=CURRENT_TIMESTAMP(6)",
    select_jobs: "SELECT j.entity_id,j.revision,
        CASE WHEN NOT s.suspended THEN JSON_OBJECT('id',s.id,'name',s.name) END AS document,NULL AS embedding
        FROM search_sync_jobs j LEFT JOIN shops s ON s.id=j.entity_id
        WHERE j.entity_type='shop' ORDER BY j.updated_at,j.entity_id LIMIT 256",
};

fn settings() -> Settings {
    Settings::new()
        .with_searchable_attributes(["name"])
        .with_filterable_attributes(["id"])
        .with_sortable_attributes(["name"])
}
