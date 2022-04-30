CREATE TABLE IF NOT EXISTS coins(
	coin BLOB PRIMARY KEY NOT NULL,
	serial BLOB NOT NULL,
	coin_blind BLOB NOT NULL,
	valcom_blind BLOB NOT NULL,
	value BLOB NOT NULL,
	network BLOB NOT NULL,
	drk_address BLOB NOT NULL,
	net_address BLOB NOT NULL,
	secret BLOB NOT NULL,
	is_spent BOOLEAN NOT NULL,
	nullifier BLOB NOT NULL,
	leaf_position BLOB NOT NULL
);
