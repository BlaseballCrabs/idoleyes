CREATE TEMPORARY TABLE webhooks_backup(url, liftcord);
INSERT INTO webhooks_backup SELECT url, liftcord FROM webhooks;
DROP TABLE webhooks;
CREATE TABLE webhooks(
    id INTEGER PRIMARY KEY,
    url TEXT UNIQUE NOT NULL,
    liftcord BOOLEAN NOT NULL
);
INSERT INTO webhooks (url, liftcord) SELECT url, liftcord FROM webhooks_backup;
DROP TABLE webhooks_backup;
