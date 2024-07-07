CREATE TABLE metadata (
    key TEXT NOT NULL PRIMARY KEY,
    value JSONB NOT NULL
);

CREATE TABLE flow (
    flow_id UUID NOT NULL PRIMARY KEY,
    destination_address TEXT NOT NULL,
    destination_port INT NOT NULL,
    protocol SMALLINT NOT NULL,
    timestamp DATETIME NOT NULL
);

CREATE TABLE message (
    message_id UUID NOT NULL PRIMARY KEY,
    flow_id UUID NOT NULL,
    kind TINYINT NOT NULL,
    timestamp DATETIME NOT NULL,
    data JSONB NOT NULL,

    FOREIGN KEY(flow_id) REFERENCES flow(flow_id)
);

CREATE TABLE artifact (
    artifact_id UUID NOT NULL PRIMARY KEY,
    from_message UUID,
    from_flow UUID,
    mime_type TEXT,
    file_name TEXT,
    timestamp DATETIME NOT NULL,
    size INT NOT NULL,
    data BLOB NOT NULL,

    FOREIGN KEY(from_message) REFERENCES flow(message_id),
    FOREIGN KEY(from_flow) REFERENCES flow(flow_id)
);
