CREATE FUNCTION getfoo(int) RETURNS SETOF users AS $$SELECT * FROM \"users\" WHERE users.id = $1;$$ LANGUAGE sql;
CREATE OR REPLACE FUNCTION getfoo(int) RETURNS SETOF users AS $$SELECT * FROM \"users\" WHERE users.id = $1;$$ LANGUAGE sql;
CREATE OR REPLACE FUNCTION getfoo(int) RETURNS SETOF users AS $$SELECT * FROM \"users\" WHERE users.id = $1;$$ LANGUAGE sql IMMUTABLE;
CREATE OR REPLACE FUNCTION getfoo(int) RETURNS SETOF users AS $$SELECT * FROM \"users\" WHERE users.id = $1;$$ LANGUAGE sql IMMUTABLE STRICT;
CREATE OR REPLACE FUNCTION getfoo(int) RETURNS SETOF users AS $$SELECT * FROM \"users\" WHERE users.id = $1;$$ LANGUAGE sql IMMUTABLE RETURNS NULL ON NULL INPUT;
CREATE OR REPLACE FUNCTION getfoo(int) RETURNS SETOF users AS $$SELECT * FROM \"users\" WHERE users.id = $1;$$ LANGUAGE sql IMMUTABLE CALLED ON NULL INPUT;
CREATE OR REPLACE FUNCTION getfoo() RETURNS text AS $$SELECT name FROM \"users\" LIMIT 1$$ LANGUAGE sql IMMUTABLE CALLED ON NULL INPUT;
