use std::{
    env,
    path::PathBuf,
    collections::HashMap
};
use structopt::StructOpt;
use openssl::ssl::{SslAcceptorBuilder, SslFiletype};
use std::{
    fs::File,
    io::Read,
    process::exit
};
use lettre::smtp::{
    authentication::{Credentials, Mechanism},
    ConnectionReuseParameters,
    SmtpClient
};
use lettre::EmailAddress;

/// The Tls credentials of a given configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TlsConfig {
    /// The TLS certificate. See the readme for instructions to generate your
    /// own.
    cert_file: PathBuf,
    /// The TLS private key file. See the readme for instructions to generate
    /// your own.
    private_key_file: PathBuf,
}

/// The configuration used to create a sysadmin account.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SysadminCreationConfig {
    /// Should a sysadmin account be created and added to the database on start.
    pub create: bool,
    /// The email to create the sysadmin account with.
    pub email: EmailAddress,
    /// The password to create the sysadmin account with.
    pub password: String
}

/// Configuration of email senders for the telescope webapp.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EmailSenderConfig {
    /// Name of the email sender.
    pub name: Option<String>,
    /// The email address of the sender.
    pub address: EmailAddress,
    /// Emails generated by the server are printed in the terminal.
    pub stub: bool,
    /// Emails generated by the server are saved to files at the given path.
    pub file: Option<PathBuf>,
    /// Emails generated by the server are send to users over SMTP.
    smtp: Option<SmtpConfig>,
}

/// Configuration of SMTP Client.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SmtpConfig {
    /// The port of the SMTP server.
    pub port: u16,
    /// The username of the email account on the server.
    /// (Part AAAA in AAAA@BBBB.CCC)
    pub username: String,
    /// The password used to login to the email account.
    pub password: String,
    /// The email server.
    /// (Part BBBB.CCC in AAAA@BBBB.CCC)
    pub host: String,
}

/// The config of the server instance.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct TelescopeConfig {
    /// Set the log level.
    /// See https://docs.rs/env_logger/0.8.1/env_logger/ for reference.
    log_level: Option<String>,
    /// Set the URL to bind the running server to.
    bind_to: Option<String>,

    /// The domain that telescope is running at.
    /// This is used to redirect callbacks to after going offsite for
    /// authentication. This is also used to generate confirmation links
    /// that get emailed to users.
    //domain: Option<String>,

    /// The URL the Postgres Database is running at.
    /// This is passed directly to diesel.
    database_url: Option<String>,

    /// The configuration of email senders.
    email_config: Option<EmailSenderConfig>,

    /// The configuration of sysadmin creation.
    sysadmin_config: Option<SysadminCreationConfig>,

    /// The TLS credential config.
    tls_config: Option<TlsConfig>,

    /// Profiles. These can be used and specified at runtime to override values
    /// defined globally. Profiles are scoped and can have sub profiles.
    profile: Option<HashMap<String, TelescopeConfig>>,
}

/// A concrete config found by searching the specified profile and parents
/// for items from the narrowest up.
///
/// The fields of this struct should match up closely to the fields of the
/// TelescopeConfig struct.
#[derive(Serialize, Debug)]
pub struct ConcreteConfig {
    pub tls_config: TlsConfig,
    log_level: String,
    pub bind_to: String,
    //pub domain: String,
    pub database_url: String,
    pub email_config: EmailSenderConfig,
    /// Sysadmin creation is not necessary to run the server.
    pub sysadmin_config: Option<SysadminCreationConfig>,
}

impl TlsConfig {
    /// Initialize
    pub fn init_tls_acceptor(&self, b: &mut SslAcceptorBuilder) {
        b.set_private_key_file(&self.private_key_file, SslFiletype::PEM)
            .expect("Could not set TLS Private Key.");
        b.set_certificate_chain_file(&self.cert_file)
            .expect("Could not set TLS Certificate.")
    }
}

impl EmailSenderConfig {
    /// Create an SMTP client if the user has specified the necessary options to
    /// do so.
    pub fn make_smtp_client(&self) -> Option<SmtpClient> {
        self.smtp.as_ref().map(|config| {
            SmtpClient::new_simple(config.host.as_str())
                .unwrap()
                .credentials(Credentials::new(
                    config.username.clone(),
                    config.password.clone(),
                ))
                .smtp_utf8(true)
                .authentication_mechanism(Mechanism::Plain)
                .connection_reuse(ConnectionReuseParameters::ReuseUnlimited)
        })
    }
}

impl TelescopeConfig {
    /// Make the profile concrete by reverse searching profiles.
    fn make_concrete(&self, profile: Vec<String>) -> ConcreteConfig {
        // check profile exists.
        let mut scope = self;
        for part in &profile {
            if scope.profile.as_ref().map(|map| map.contains_key(part)).unwrap_or(false) {
                scope = scope.profile.as_ref().unwrap().get(part).unwrap();
            } else {
                eprintln!("Profile path {:?} not found in config. missing part {}.", profile, part);
                exit(1)
            }
        }

        let profile_slice = &profile[..];
        ConcreteConfig {
            tls_config: self.reverse_lookup(profile_slice, |c| c.tls_config.clone())
                .expect("Could not resolve TLS config."),
            log_level: self.reverse_lookup(profile_slice, |c| c.log_level.clone())
                .expect("Could not resolve log level."),
            bind_to: self.reverse_lookup(profile_slice, |c| c.bind_to.clone())
                .expect("Could not resolve binding URL."),
            //domain: self.reverse_lookup(profile_slice, |c| c.domain.clone())
            //    .expect("Could not resolve domain configuration."),
            database_url: self.reverse_lookup(profile_slice, |c| c.database_url.clone())
                .expect("Could not resolve database URL."),
            email_config: self.reverse_lookup(profile_slice, |c| c.email_config.clone())
                .expect("Could not resolve email config."),
            sysadmin_config: self.reverse_lookup(profile_slice, |c| c.sysadmin_config.clone())
        }
    }

    /// Reverse lookup a property using an extractor.
    ///
    /// Assume profile is valid and exists.
    fn reverse_lookup<T: Clone>(&self, profile_slice: &[String], extractor: impl Fn(&Self) -> Option<T> + Copy) -> Option<T> {
        if profile_slice.len() >= 2 {
            let child_path = &profile_slice[1..];
            let child = self.profile.as_ref().unwrap().get(&profile_slice[0]).unwrap();
            child.reverse_lookup(child_path, extractor).or(extractor(self))
        }
        else if profile_slice.len() == 1 {
            extractor(self.profile.as_ref().unwrap().get(&profile_slice[0]).unwrap())
                .or(extractor(self))
        } else {
            extractor(self)
        }
    }
}

// The name, about, version, and authors are given by cargo.
/// Stores the configuration of the telescope server. An instance of this is created and stored in
/// a lazy static before the server is launched.
#[derive(Debug, Serialize, StructOpt)]
#[structopt(about = "The RCOS webapp", rename_all = "screaming-snake")]
struct CommandLine {
    /// The config file for this Telescope instance. See config_example.toml
    /// for more details.
    #[structopt(short = "c", long = "config", env, default_value = "config.toml")]
    config_file: PathBuf,
    /// What profile (if any) to use from the config file.
    ///
    /// Subprofiles can be specified using a '.' delimiter, e.g.
    /// 'dev.create_sysadmin'
    #[structopt(short = "p", long = "profile", env)]
    profile: Option<String>
}

lazy_static! {
    /// Global web server configuration.
    pub static ref CONFIG: ConcreteConfig = cli();
}

/// After the global configuration is initialized, log it as info.
pub fn init() {
    let cfg: &ConcreteConfig = &*CONFIG;

    // initialize logger.
    env_logger::builder()
        .parse_filters(&cfg.log_level)
        .init();

    info!("Starting up...");
    info!("telescope {}", env!("CARGO_PKG_VERSION"));
    trace!("Config: \n{}", serde_json::to_string_pretty(cfg).unwrap());
}

/// Digest and handle arguments from the command line. Read arguments from environment
/// variables where necessary. Construct and return the configuration specified.
/// Initializes logging and returns config.
fn cli() -> ConcreteConfig {
    // set env vars from a ".env" file if available.
    dotenv::dotenv().ok();

    let commandline: CommandLine = CommandLine::from_args();

    let mut confing_file_string = String::new();

    File::open(&commandline.config_file)
        .map_err(|e| {
            eprintln!("Could not open config file at {}: {}", commandline.config_file.display(), e);
            e
        })
        .unwrap()
        .read_to_string(&mut confing_file_string)
        .map_err(|e| {
            eprintln!("Could not read config file at {}: {}", commandline.config_file.display(), e);
            e
        })
        .unwrap();
    let parsed = toml::from_str::<TelescopeConfig>(confing_file_string.as_str())
        .map_err(|e| {
            eprintln!("Error deserializing config file: {}", e);
            e
        })
        .unwrap();

    let profile_path = commandline.profile
        .map(|s| s.split(".").map(|p| p.to_string()).collect())
        .unwrap_or(Vec::new());

    parsed.make_concrete(profile_path)
}
