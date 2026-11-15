-- The PostGIS tiger geocoder and topology extensions (and fuzzystrmatch, which the geocoder needs) came with
-- the Postgres image; nothing uses them. They were dropped by hand on the Pi on 2026-11-15 (the app now has an
-- ordinary role, which cannot drop an extension), so on that database this migration does nothing; it makes
-- fresh and test databases match it.
-- Rollback (as the superuser): CREATE EXTENSION fuzzystrmatch; CREATE EXTENSION postgis_tiger_geocoder;
-- CREATE EXTENSION postgis_topology;
DROP EXTENSION IF EXISTS postgis_tiger_geocoder;
DROP EXTENSION IF EXISTS postgis_topology;
DROP EXTENSION IF EXISTS fuzzystrmatch;
DROP SCHEMA IF EXISTS tiger_data;
DROP SCHEMA IF EXISTS tiger;
DROP SCHEMA IF EXISTS topology;
