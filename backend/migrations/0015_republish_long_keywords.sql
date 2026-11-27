-- Republish existing titles that exceed the compact autocomplete limits.
UPDATE search_keyword_terms
SET dirty=TRUE
WHERE CHAR_LENGTH(phrase)>72
   OR CHAR_LENGTH(phrase)-CHAR_LENGTH(REPLACE(phrase,' ',''))>=8;
