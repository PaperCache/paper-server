/*
 * Copyright (c) Kia Shakiba
 *
 * This source code is licensed under the GNU AGPLv3 license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::{
	io::Write,
	net::{Shutdown, TcpListener, TcpStream},
	str::FromStr,
	sync::{
		Arc,
		atomic::{AtomicUsize, Ordering},
	},
};

use kwik::thread_pool::ThreadPool;
use log::{error, info, warn};
use paper_cache::{CacheError, PaperCache, PaperPolicy};
use paper_utils::{
	sheet::{Sheet, SheetBuilder},
	stream::Buffer,
};

use crate::{command::Command, config::Config, connection::Connection, error::ServerError};

pub type Cache = PaperCache<Buffer, Buffer>;
type SheetResult = Result<Sheet, ServerError>;

pub struct Server {
	listener: TcpListener,
	cache:    Arc<Cache>,

	pool: ThreadPool,

	max_connections: usize,
	num_connections: Arc<AtomicUsize>,
	auth_token:      Option<u64>,
}

impl Server {
	pub fn new(config: &Config, cache: Cache) -> Result<Self, ServerError> {
		let addr = format!("{}:{}", config.host(), config.port());

		let Ok(listener) = TcpListener::bind(addr) else {
			return Err(ServerError::InvalidAddress);
		};

		let server = Server {
			listener,
			cache: Arc::new(cache),

			pool: ThreadPool::new(config.max_connections()),

			max_connections: config.max_connections(),
			num_connections: Arc::new(AtomicUsize::new(0)),
			auth_token: config.auth_token(),
		};

		Ok(server)
	}

	pub fn listen(&mut self) -> Result<(), ServerError> {
		for stream in self.listener.incoming() {
			match stream {
				Ok(mut stream) => {
					if self.num_connections.load(Ordering::Relaxed) == self.max_connections {
						warn!("Maximum number of connections exceeded");

						max_connections_reject_handshake(&mut stream)?;

						let _ = stream.shutdown(Shutdown::Both);
						return Err(ServerError::MaxConnectionsExceeded);
					}

					let address = stream
						.peer_addr()
						.map(|address| address.to_string())
						.unwrap_or("-1".into());

					info!("Connected: {address}");

					success_handshake(&mut stream)?;

					let connection = Connection::new(stream, self.auth_token);
					let cache = self.cache.clone();
					let num_connections = Arc::clone(&self.num_connections);

					self.pool.execute(move || {
						num_connections.fetch_add(1, Ordering::Relaxed);
						Server::handle_connection(connection, cache);

						info!("Disconnected: {address}");
						num_connections.fetch_sub(1, Ordering::Relaxed);
					});
				},

				Err(_) => return Err(ServerError::InvalidConnection),
			}
		}

		Ok(())
	}

	fn handle_connection(mut connection: Connection, cache: Arc<Cache>) {
		loop {
			let command = match connection.get_command() {
				Ok(command) => command,
				Err(ServerError::Disconnected) => return,

				Err(err) => {
					error!("{err}");
					continue;
				},
			};

			let sheet_result = match (connection.is_authorized(), command) {
				(_, Command::Ping) => handle_ping(),
				(_, Command::Version) => handle_version(&cache),

				(_, Command::Auth(token)) => handle_auth(&mut connection, &token),

				(true, Command::Get(key)) => handle_get(&cache, key),
				(true, Command::Set(key, value, ttl)) => handle_set(&cache, key, value, ttl),
				(true, Command::Del(key)) => handle_del(&cache, key),

				(true, Command::Has(key)) => handle_has(&cache, key),
				(true, Command::Peek(key)) => handle_peek(&cache, key),
				(true, Command::Ttl(key, ttl)) => handle_ttl(&cache, key, ttl),
				(true, Command::Size(key)) => handle_size(&cache, key),

				(true, Command::Wipe) => handle_wipe(&cache),

				(true, Command::Resize(size)) => handle_resize(&cache, size),
				(true, Command::Policy(policy_str)) => handle_policy(&cache, policy_str),

				(true, Command::Status) => handle_status(&cache),

				_ => Err(ServerError::Unauthorized),
			};

			let sheet = sheet_result.unwrap_or_else(|err| err.to_sheet());

			if (connection.send_response(sheet.serialize())).is_err() {
				error!("Could not send response to command");
			}
		}
	}
}

fn success_handshake(stream: &mut TcpStream) -> Result<(), ServerError> {
	let sheet = SheetBuilder::new().write_bool(true).into_sheet();

	stream
		.write_all(sheet.serialize())
		.map_err(|_| ServerError::InvalidResponse)
}

fn max_connections_reject_handshake(stream: &mut TcpStream) -> Result<(), ServerError> {
	let sheet = ServerError::MaxConnectionsExceeded.to_sheet();

	stream
		.write_all(sheet.serialize())
		.map_err(|_| ServerError::InvalidResponse)
}

fn handle_ping() -> SheetResult {
	let sheet = SheetBuilder::new()
		.write_bool(true)
		.write_buf(b"pong")
		.into_sheet();

	Ok(sheet)
}

fn handle_version(cache: &Arc<Cache>) -> SheetResult {
	let sheet = SheetBuilder::new()
		.write_bool(true)
		.write_str(cache.version())
		.into_sheet();

	Ok(sheet)
}

fn handle_auth(connection: &mut Connection, token: &Buffer) -> SheetResult {
	let is_authorized =
		String::from_utf8(token.to_vec()).is_ok_and(|token| connection.authorize(&token));

	if !is_authorized {
		return Err(ServerError::Unauthorized);
	}

	let sheet = SheetBuilder::new().write_bool(true).into_sheet();

	Ok(sheet)
}

fn handle_get(cache: &Arc<Cache>, key: Buffer) -> SheetResult {
	cache
		.get(&key)
		.map(|object| {
			SheetBuilder::new()
				.write_bool(true)
				.write_buf(&object)
				.into_sheet()
		})
		.map_err(ServerError::CacheError)
}

fn handle_set(cache: &Arc<Cache>, key: Buffer, value: Buffer, ttl: Option<u32>) -> SheetResult {
	cache
		.set(key, value, ttl)
		.map(|_| SheetBuilder::new().write_bool(true).into_sheet())
		.map_err(ServerError::CacheError)
}

fn handle_del(cache: &Arc<Cache>, key: Buffer) -> SheetResult {
	cache
		.del(&key)
		.map(|_| SheetBuilder::new().write_bool(true).into_sheet())
		.map_err(ServerError::CacheError)
}

fn handle_has(cache: &Arc<Cache>, key: Buffer) -> SheetResult {
	let sheet = SheetBuilder::new()
		.write_bool(true)
		.write_bool(cache.has(&key))
		.into_sheet();

	Ok(sheet)
}

fn handle_peek(cache: &Arc<Cache>, key: Buffer) -> SheetResult {
	cache
		.peek(&key)
		.map(|object| {
			SheetBuilder::new()
				.write_bool(true)
				.write_buf(&object)
				.into_sheet()
		})
		.map_err(ServerError::CacheError)
}

fn handle_ttl(cache: &Arc<Cache>, key: Buffer, ttl: Option<u32>) -> SheetResult {
	cache
		.ttl(&key, ttl)
		.map(|_| SheetBuilder::new().write_bool(true).into_sheet())
		.map_err(ServerError::CacheError)
}

fn handle_size(cache: &Arc<Cache>, key: Buffer) -> SheetResult {
	cache
		.size(&key)
		.map(|size| {
			SheetBuilder::new()
				.write_bool(true)
				.write_u32(size)
				.into_sheet()
		})
		.map_err(ServerError::CacheError)
}

fn handle_wipe(cache: &Arc<Cache>) -> SheetResult {
	cache
		.wipe()
		.map(|_| SheetBuilder::new().write_bool(true).into_sheet())
		.map_err(ServerError::CacheError)
}

fn handle_resize(cache: &Arc<Cache>, size: u64) -> SheetResult {
	cache
		.resize(size)
		.map(|_| SheetBuilder::new().write_bool(true).into_sheet())
		.map_err(ServerError::CacheError)
}

fn handle_policy(cache: &Arc<Cache>, policy_str: String) -> SheetResult {
	let Ok(policy) = PaperPolicy::from_str(&policy_str) else {
		return Err(ServerError::CacheError(CacheError::InvalidPolicy));
	};

	cache
		.policy(policy)
		.map(|_| SheetBuilder::new().write_bool(true).into_sheet())
		.map_err(ServerError::CacheError)
}

fn handle_status(cache: &Arc<Cache>) -> SheetResult {
	let status = cache.status().map_err(ServerError::CacheError)?;

	let mut sheet_builder = SheetBuilder::new()
		.write_bool(true)
		.write_u32(status.pid())
		.write_u64(status.max_size())
		.write_u64(status.used_size())
		.write_u64(status.num_objects())
		.write_u64(status.rss())
		.write_u64(status.hwm())
		.write_u64(status.total_gets())
		.write_u64(status.total_sets())
		.write_u64(status.total_dels())
		.write_f64(status.miss_ratio())
		.write_u32(status.policies().len() as u32);

	for policy in status.policies() {
		sheet_builder = sheet_builder.write_str(policy.to_string());
	}

	let sheet = sheet_builder
		.write_str(status.policy().to_string())
		.write_bool(status.is_auto_policy())
		.write_u64(status.uptime())
		.into_sheet();

	Ok(sheet)
}
