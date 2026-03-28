/*
 * Copyright (c) Kia Shakiba
 *
 * This source code is licensed under the GNU AGPLv3 license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::{
	hash::{DefaultHasher, Hash, Hasher},
	io::Write,
	net::{Shutdown, TcpStream},
};

use paper_utils::stream::StreamError;

use crate::{command::Command, error::ServerError};

pub struct Connection {
	stream: TcpStream,

	auth_token:    Option<u64>,
	is_authorized: bool,
}

impl Connection {
	pub fn new(stream: TcpStream, auth_token: Option<u64>) -> Self {
		let is_authorized = auth_token.is_none();

		Connection {
			stream,

			auth_token,
			is_authorized,
		}
	}

	pub fn close(&self) -> Result<(), ServerError> {
		self.stream
			.shutdown(Shutdown::Both)
			.map_err(|_| ServerError::Internal)
	}

	pub fn is_authorized(&self) -> bool {
		self.is_authorized
	}

	pub fn authorize(&mut self, value: &str) -> bool {
		if self.is_authorized {
			return true;
		}

		let mut s = DefaultHasher::new();
		value.hash(&mut s);

		self.is_authorized = self
			.auth_token
			.is_some_and(|token| token == s.finish());

		self.is_authorized
	}

	pub fn get_command(&mut self) -> Result<Command, ServerError> {
		Command::from_stream(&mut self.stream).map_err(|err| match err {
			StreamError::InvalidStream | StreamError::ClosedStream => ServerError::Disconnected,

			_ => ServerError::InvalidCommand(err.to_string()),
		})
	}

	pub fn send_response(&mut self, buf: &[u8]) -> Result<(), ServerError> {
		self.stream
			.write_all(buf)
			.map_err(|_| ServerError::InvalidResponse)
	}
}
