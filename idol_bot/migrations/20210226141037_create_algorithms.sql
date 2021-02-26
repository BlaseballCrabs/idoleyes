CREATE TABLE algorithms(
    algorithm INTEGER NOT NULL,
    joke BOOLEAN NOT NULL,
    webhook_id INTEGER NOT NULL REFERENCES webhook(id) ON DELETE CASCADE
);
