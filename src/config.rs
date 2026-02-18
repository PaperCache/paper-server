/*
 * Copyright (c) Kia Shakiba
 *
 * This source code is licensed under the GNU AGPLv3 license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::{
	env,
	hash::{DefaultHasher, Hash, Hasher},
	include_str,
	path::Path,
	str::FromStr,
};

use kwik::file::{FileReader, text::TextReader};
use paper_cache::PaperPolicy;
use parse_size::parse_size;

use crate::error::ServerError;

#[derive(Debug)]
pub struct Config {
	host: String,
	port: u32,

	max_size: u64,
	policies: Vec<PaperPolicy>,
	policy:   PaperPolicy,

	max_connections: usize,
	auth_token:      Option<u64>,
}

enum ConfigValue {
	Host(String),
	Port(u32),

	MaxSize(u64),
	PoliciesItem(PaperPolicy),
	Policy(PaperPolicy),

	MaxConnections(usize),
	AuthToken(u64),
}

impl Config {
	pub fn from_file<P>(path: P) -> Result<Self, ServerError>
	where
		P: AsRef<Path>,
	{
		let reader = match TextReader::from_path(path) {
			Ok(reader) => reader,
			Err(_) => return Err(ServerError::InvalidConfig),
		};

		let mut config = init_uninitialized_config();

		let file_iter = reader
			.into_iter()
			.map(|line| line.trim().to_owned())
			.filter(|line| !line.is_empty() && !line.starts_with('#'));

		for line in file_iter {
			Config::parse_line(&mut config, &line)?;
		}

		Ok(config)
	}

	pub fn host(&self) -> &str {
		&self.host
	}

	pub fn port(&self) -> u32 {
		self.port
	}

	pub fn max_size(&self) -> u64 {
		self.max_size
	}

	pub fn policies(&self) -> &[PaperPolicy] {
		&self.policies
	}

	pub fn policy(&self) -> PaperPolicy {
		self.policy
	}

	pub fn max_connections(&self) -> usize {
		self.max_connections
	}

	pub fn auth_token(&self) -> Option<u64> {
		self.auth_token
	}

	fn parse_line(config: &mut Config, line: &str) -> Result<(), ServerError> {
		let tokens: Vec<&str> = line.split('=').collect();

		if tokens.len() != 2 {
			return Err(ServerError::InvalidConfigLine(line.into()));
		}

		let token_value = try_parse_env(tokens[1]).unwrap_or(tokens[1].into());

		let config_value = match tokens[0] {
			"host" => parse_host(&token_value),
			"port" => parse_port(&token_value),

			"max_size" => parse_max_size(&token_value),
			"policies[]" => parse_policies_item(&token_value),
			"policy" => parse_policy(&token_value),

			"max_connections" => parse_max_connections(&token_value),
			"auth_token" => parse_auth_token(&token_value),

			_ => Err(ServerError::InvalidConfigLine(line.into())),
		};

		match config_value {
			Ok(value) => match value {
				ConfigValue::Host(host) => config.host = host,
				ConfigValue::Port(port) => config.port = port,

				ConfigValue::MaxSize(max_size) => config.max_size = max_size,
				ConfigValue::PoliciesItem(policy) => config.policies.push(policy),
				ConfigValue::Policy(policy) => config.policy = policy,

				ConfigValue::MaxConnections(max_connections) => {
					config.max_connections = max_connections
				},
				ConfigValue::AuthToken(token) => config.auth_token = Some(token),
			},

			Err(err) => return Err(err),
		}

		Ok(())
	}
}

impl Default for Config {
	fn default() -> Self {
		let default_config_data = include_str!("../default.pconf");
		let mut config = init_uninitialized_config();

		let line_iter = default_config_data
			.split('\n')
			.map(|line| line.trim().to_owned())
			.filter(|line| !line.is_empty() && !line.starts_with('#'));

		for line in line_iter {
			Config::parse_line(&mut config, &line)
				.expect("An error occured when parsing default config");
		}

		config
	}
}

fn init_uninitialized_config() -> Config {
	Config {
		host: String::new(),
		port: 0,

		max_size: 0,
		policies: Vec::new(),
		policy:   PaperPolicy::Lfu,

		max_connections: 0,
		auth_token:      None,
	}
}

fn try_parse_env(value: &str) -> Option<String> {
	let value = value.trim();

	match value.starts_with('$') {
		true => env::var(&value[1..]).ok(),
		false => None,
	}
}

fn parse_host(value: &str) -> Result<ConfigValue, ServerError> {
	if value.is_empty() {
		return Err(ServerError::InvalidConfigParam("host"));
	}

	Ok(ConfigValue::Host(value.to_owned()))
}

fn parse_port(value: &str) -> Result<ConfigValue, ServerError> {
	match value.parse::<u32>() {
		Ok(value) => Ok(ConfigValue::Port(value)),
		Err(_) => Err(ServerError::InvalidConfigParam("port")),
	}
}

fn parse_max_size(value: &str) -> Result<ConfigValue, ServerError> {
	match parse_size(value) {
		Ok(0) | Err(_) => Err(ServerError::InvalidConfigParam("max_size")),
		Ok(value) => Ok(ConfigValue::MaxSize(value)),
	}
}

fn parse_policies_item(value: &str) -> Result<ConfigValue, ServerError> {
	match PaperPolicy::from_str(value) {
		Ok(policy) if !policy.is_auto() => Ok(ConfigValue::PoliciesItem(policy)),
		_ => Err(ServerError::InvalidConfigPolicy(value.into())),
	}
}

fn parse_policy(value: &str) -> Result<ConfigValue, ServerError> {
	match PaperPolicy::from_str(value) {
		Ok(policy) => Ok(ConfigValue::Policy(policy)),
		Err(_) => Err(ServerError::InvalidConfigPolicy(value.into())),
	}
}

fn parse_max_connections(value: &str) -> Result<ConfigValue, ServerError> {
	match value.parse::<usize>() {
		Ok(0) | Err(_) => Err(ServerError::InvalidConfigParam("max_connections")),
		Ok(value) => Ok(ConfigValue::MaxConnections(value)),
	}
}

fn parse_auth_token(value: &str) -> Result<ConfigValue, ServerError> {
	if value.is_empty() {
		return Err(ServerError::InvalidConfigParam("auth_token"));
	}

	let mut s = DefaultHasher::new();
	value.hash(&mut s);

	Ok(ConfigValue::AuthToken(s.finish()))
}
