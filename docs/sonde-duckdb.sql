-- Sonde en lecture seule d'un fichier de télémétrie écrit par Le Mans Ultimate (DuckDB, dossier UserData\Telemetry).
-- Ne lit que ce fichier (contenu d'Alex) ; n'écrit rien dedans (-readonly). Aucun fichier de physique (.VEH / .mas).
-- Lancer dans un dossier de travail (pas dans UserData) :
--   duckdb -readonly "C:\...\UserData\Telemetry\<séance>.duckdb" -c ".read sonde-duckdb.sql"
-- Produit 4 fichiers à envoyer : sonde-duckdb-tables.json, -colonnes.json, -comptes.json, -apercu.json.

.mode json
.output sonde-duckdb-tables.json
SELECT schema_name, table_name, estimated_size, column_count FROM duckdb_tables() ORDER BY schema_name, table_name;

.output sonde-duckdb-colonnes.json
SELECT table_name, column_index, column_name, data_type FROM duckdb_columns() WHERE NOT internal ORDER BY table_name, column_index;

-- Requêtes générées pour chaque table : nombre exact de lignes, plage de la première colonne de temps
-- (nom contenant time, ts ou stamp), aperçu des 3 premières lignes en JSON.
.mode list
.headers off
.output sonde-duckdb-gen.sql
SELECT '.mode json';
SELECT '.output sonde-duckdb-comptes.json';
SELECT string_agg(q, ' UNION ALL ') || ';' FROM (
  SELECT format('SELECT {} AS table_name, count(*) AS lignes, {} AS colonne_temps, {} AS temps_min, {} AS temps_max FROM "{}"',
    '''' || t.table_name || '''',
    coalesce('''' || c.column_name || '''', 'NULL'),
    coalesce('CAST(min("' || c.column_name || '") AS VARCHAR)', 'NULL'),
    coalesce('CAST(max("' || c.column_name || '") AS VARCHAR)', 'NULL'),
    t.table_name) AS q
  FROM duckdb_tables() t
  LEFT JOIN (
    SELECT table_name, arg_min(column_name, column_index) AS column_name FROM duckdb_columns()
    WHERE NOT internal AND (lower(column_name) LIKE '%time%' OR lower(column_name) = 'ts' OR lower(column_name) LIKE '%stamp%')
    GROUP BY table_name
  ) c USING (table_name)
  ORDER BY t.table_name
);
SELECT '.output sonde-duckdb-apercu.json';
SELECT string_agg(q, ' UNION ALL ') || ';' FROM (
  SELECT format('(SELECT {} AS table_name, CAST(to_json(x) AS VARCHAR) AS ligne FROM "{}" x LIMIT 3)', '''' || table_name || '''', table_name) AS q
  FROM duckdb_tables() ORDER BY table_name
);
.output
.read sonde-duckdb-gen.sql
