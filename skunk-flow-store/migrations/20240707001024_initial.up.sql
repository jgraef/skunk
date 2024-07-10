-- .flows file format

-- global metadata
CREATE TABLE metadata (
    key TEXT NOT NULL PRIMARY KEY,
    value JSONB NOT NULL
);


-- flows
CREATE TABLE flow (
    flow_id UUID NOT NULL PRIMARY KEY,
    parent_id UUID,
    protocol TEXT,
    timestamp DATETIME NOT NULL,
    metadata JSONB,

    FOREIGN KEY(parent_id) REFERENCES flow(flow_id)
);

CREATE INDEX index_flow_protocol ON flow(protocol);
CREATE INDEX index_flow_timestamp ON flow(timestamp);


-- messages
CREATE TABLE message (
    message_id UUID NOT NULL PRIMARY KEY,
    flow_id UUID NOT NULL,
    kind TINYINT NOT NULL,
    timestamp DATETIME NOT NULL,
    data JSONB NOT NULL,
    metadata JSONB,

    FOREIGN KEY(flow_id) REFERENCES flow(flow_id)
);

CREATE INDEX index_message_timestamp ON message(timestamp);


-- artifacts
CREATE TABLE artifact (
    artifact_id UUID NOT NULL PRIMARY KEY,
    message_id UUID,
    mime_type TEXT,
    file_name TEXT,
    timestamp DATETIME NOT NULL,
    hash BLOB NOT NULL,

    FOREIGN KEY(message_id) REFERENCES flow(message_id),
    FOREIGN KEY(hash) REFERENCES artifact_blob(hash)
);

CREATE INDEX index_artifact_mime_type ON artifact(mime_type);
CREATE INDEX index_artifact_file_name ON artifact(file_name);
CREATE INDEX index_artifact_timestamp ON artifact(timestamp);

CREATE TABLE artifact_blob (
    hash BLOB NOT NULL PRIMARY KEY,
    size INT NOT NULL,
    data BLOB NOT NULL
);
