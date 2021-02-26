CREATE TABLE webhooks_backup(
    id INTEGER PRIMARY KEY,
    url TEXT UNIQUE NOT NULL
);
INSERT INTO webhooks_backup SELECT id, url FROM webhooks;
DROP TABLE webhooks;
CREATE TABLE webhooks(
    id INTEGER PRIMARY KEY,
    url TEXT UNIQUE NOT NULL
);
INSERT INTO webhooks SELECT id, url FROM webhooks_backup;
DROP TABLE webhooks_backup;
