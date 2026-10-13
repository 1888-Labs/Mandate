//! `mandate`: grant, delegate, check and revoke authority for AI agents.
//!
//! Exit codes: 0 success / ALLOW, 1 DENY or rejected delegation, 2 error.

mod commands;
mod error;
mod output;
mod state;
mod time;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use commands::Outcome;
use error::CliError;

#[derive(Parser)]
#[command(
    name = "mandate",
    version,
    about = "Verifiable delegated authorization for AI agents",
    after_help = "Exit codes: 0 success or ALLOW, 1 DENY or rejected delegation, 2 error."
)]
struct Cli {
    /// Directory for local state and development keys.
    #[arg(long, global = true, env = "MANDATE_HOME", default_value = ".mandate")]
    home: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create and list principals (humans and agents).
    #[command(subcommand)]
    Principal(PrincipalCommand),

    /// Register who owns a resource (who may issue root mandates for it).
    #[command(subcommand)]
    Resource(ResourceCommand),

    /// Issue a root mandate as the owner of a resource.
    Grant(GrantArgs),

    /// Delegate a narrower slice of an existing mandate.
    Delegate(DelegateArgs),

    /// Ask whether an agent may perform an action. Exits 0 on ALLOW, 1 on DENY.
    Check(CheckArgs),

    /// Revoke a mandate, as its issuer. Everything delegated beneath it stops working.
    Revoke {
        /// Mandate alias (M1), full ID or ID prefix.
        mandate: String,
        /// Reason recorded in the signed revocation.
        #[arg(long, default_value = "user_revoked")]
        reason: String,
    },

    /// Show a mandate's fields, signature and status.
    Inspect {
        /// Mandate alias (M1), full ID or ID prefix.
        mandate: String,
    },

    /// Show the delegation chain from the root to a mandate.
    Chain {
        /// Mandate alias (M1), full ID or ID prefix.
        mandate: String,
    },

    /// Print a mandate's signed JSON document, exactly as stored and published.
    Export {
        /// Mandate alias (M1), full ID or ID prefix.
        mandate: String,
        /// Print the signed revocations of this mandate instead (a JSON array).
        #[arg(long)]
        revocations: bool,
    },
}

#[derive(Subcommand)]
enum PrincipalCommand {
    /// Create a principal with a new development keypair.
    Create { name: String },
    /// List principals.
    List,
}

#[derive(Subcommand)]
enum ResourceCommand {
    /// Trust a principal to issue root mandates for a resource.
    Own {
        /// Resource as kind:id, e.g. wallet:treasury-demo.
        resource: String,
        #[arg(long)]
        owner: String,
    },
    /// List resources and their owners.
    List,
}

#[derive(Args)]
struct GrantArgs {
    #[arg(long)]
    from: String,
    #[arg(long)]
    to: String,
    #[arg(long)]
    action: String,
    /// Resource as kind:id, e.g. wallet:treasury-demo.
    #[arg(long)]
    resource: String,
    #[arg(long)]
    max_amount: Option<u64>,
    /// Allowed recipient. Repeat for several. Omit for no restriction.
    #[arg(long = "recipient")]
    recipients: Vec<String>,
    /// Allow the subject to delegate further.
    #[arg(long)]
    delegable: bool,
    /// Lifetime, e.g. 48h, 30m, 7d.
    #[arg(long)]
    expires_in: String,
}

#[derive(Args)]
struct DelegateArgs {
    /// Parent mandate: alias (M1), full ID or ID prefix.
    #[arg(long)]
    mandate: String,
    #[arg(long)]
    from: String,
    #[arg(long)]
    to: String,
    /// Defaults to the parent's limit.
    #[arg(long)]
    max_amount: Option<u64>,
    /// Allowed recipient. Repeat for several. Defaults to the parent's.
    #[arg(long = "recipient")]
    recipients: Vec<String>,
    /// Allow the new subject to delegate further.
    #[arg(long)]
    delegable: bool,
    /// Lifetime, e.g. 24h. Defaults to the parent's expiry.
    #[arg(long)]
    expires_in: Option<String>,
}

#[derive(Args)]
struct CheckArgs {
    #[arg(long)]
    agent: String,
    #[arg(long)]
    action: String,
    #[arg(long)]
    resource: String,
    #[arg(long)]
    amount: Option<u64>,
    #[arg(long)]
    recipient: Option<String>,
    /// Evaluate at this Unix time instead of now.
    #[arg(long)]
    at: Option<u64>,
}

fn run(cli: Cli) -> Result<Outcome, CliError> {
    let home = cli.home.as_path();
    match cli.command {
        Command::Principal(PrincipalCommand::Create { name }) => {
            commands::principal_create(home, &name)
        }
        Command::Principal(PrincipalCommand::List) => commands::principal_list(home),
        Command::Resource(ResourceCommand::Own { resource, owner }) => {
            commands::resource_own(home, &resource, &owner)
        }
        Command::Resource(ResourceCommand::List) => commands::resource_list(home),
        Command::Grant(a) => commands::grant(
            home,
            commands::GrantArgs {
                from: a.from,
                to: a.to,
                action: a.action,
                resource: a.resource,
                max_amount: a.max_amount,
                recipients: a.recipients,
                delegable: a.delegable,
                expires_in: a.expires_in,
            },
        ),
        Command::Delegate(a) => commands::delegate(
            home,
            commands::DelegateArgs {
                mandate: a.mandate,
                from: a.from,
                to: a.to,
                max_amount: a.max_amount,
                recipients: a.recipients,
                delegable: a.delegable,
                expires_in: a.expires_in,
            },
        ),
        Command::Check(a) => commands::check(
            home,
            commands::CheckArgs {
                agent: a.agent,
                action: a.action,
                resource: a.resource,
                amount: a.amount,
                recipient: a.recipient,
                at: a.at,
            },
        ),
        Command::Revoke { mandate, reason } => commands::revoke(home, &mandate, reason),
        Command::Inspect { mandate } => commands::inspect(home, &mandate),
        Command::Chain { mandate } => commands::chain(home, &mandate),
        Command::Export {
            mandate,
            revocations,
        } => commands::export(home, &mandate, revocations),
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(Outcome::Ok) => ExitCode::SUCCESS,
        Ok(Outcome::Denied) => ExitCode::from(1),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}
