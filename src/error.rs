/*
 * Copyright (c) Kia Shakiba
 *
 * This source code is licensed under the GNU AGPLv3 license found in the
 * LICENSE file in the root directory of this source tree.
 */

use paper_cache::CacheError;
use paper_utils::sheet::{Sheet, SheetBuilder};
use thiserror::Error;

#[derive(Debug, PartialEq, Error)]
pub enum ServerError {
	#[error(transparent)]
	CacheError(#[from] CacheError),

	#[error("could not establish a connection")]
	InvalidAddress,

	#[error("could not establish a connection")]
	InvalidConnection,

	#[error("the maximum number of connections was exceeded")]
	MaxConnectionsExceeded,

	#[error("{0}")]
	InvalidCommand(String),

	#[error("invalid response")]
	InvalidResponse,

	#[error("disconnected from client")]
	Disconnected,

	#[error("could not open config file")]
	InvalidConfig,

	#[error("invalid config line <{0}>")]
	InvalidConfigLine(String),

	#[error("invalid {0} config")]
	InvalidConfigParam(&'static str),

	#[error("invalid policy <{0}> in config")]
	InvalidConfigPolicy(String),

	#[error("unauthorized")]
	Unauthorized,
}

impl ServerError {
	pub fn to_sheet(&self) -> Sheet {
		if let ServerError::CacheError(err) = self {
			return SheetBuilder::new()
				.write_bool(false)
				.write_u8(get_error_code(self))
				.write_u8(get_cache_error_code(err))
				.into_sheet();
		}

		SheetBuilder::new()
			.write_bool(false)
			.write_u8(get_error_code(self))
			.into_sheet()
	}
}

fn get_error_code(error: &ServerError) -> u8 {
	match error {
		ServerError::CacheError(_) => 0,

		ServerError::InvalidAddress
		| ServerError::InvalidConnection
		| ServerError::InvalidCommand(_)
		| ServerError::InvalidResponse
		| ServerError::Disconnected
		| ServerError::InvalidConfig
		| ServerError::InvalidConfigLine(_)
		| ServerError::InvalidConfigParam(_)
		| ServerError::InvalidConfigPolicy(_) => 1,

		ServerError::MaxConnectionsExceeded => 2,
		ServerError::Unauthorized => 3,
	}
}

fn get_cache_error_code(error: &CacheError) -> u8 {
	match error {
		CacheError::KeyNotFound => 1,

		CacheError::ZeroValueSize => 2,
		CacheError::ExceedingValueSize => 3,

		CacheError::ZeroCacheSize => 4,

		CacheError::UnconfiguredPolicy => 5,
		CacheError::InvalidPolicy => 6,

		_ => 0,
	}
}
