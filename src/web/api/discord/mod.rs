//! Discord API utilities and serenity tie-ins.

mod event_handler;
mod init;

use event_handler::Handler;
use serenity::client::Client;
use actix::{Actor, Context, AsyncContext, ActorFuture, SpawnHandle};
use crate::env::{global_config, DiscordConfig};
use serenity::model::interactions::{Interaction, ApplicationCommandOptionType};
use serenity::builder::{CreateInteractionOption, CreateInteraction};
use std::pin::Pin;
use std::task::Poll;
use futures::Future;
use serenity::model::id::GuildId;

/// Future wrapper to initialize serenity in an actix future.
struct InitSerenityFuture<F: Future<Output = Client> + std::marker::Unpin + 'static> {
    inner: F
}

impl<F: Future<Output = Client> + std::marker::Unpin> ActorFuture for InitSerenityFuture<F>
{
    type Output = ();
    type Actor = DiscordActor;

    fn poll(mut self: Pin<&mut Self>, srv: &mut DiscordActor, _: &mut <DiscordActor as Actor>::Context, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        // Get a pin on the mutable pointer to the initialization future.
        let inner: Pin<&mut F> = Pin::new(&mut self.inner);

        // Poll the inner future.
        match Future::poll(inner, cx) {
            // If the inner future is ready, add the client to the actor and return ready.
            Poll::Ready(serenity_client) => {
                srv.serenity_client = Some(serenity_client);
                return Poll::Ready(());
            },

            // Otherwise, keep waiting on the internal future.
            Poll::Pending => Poll::Pending
        }
    }
}

/// Future wrapper storing that never resolves while serenity's
/// shards are running.
struct SerenityListeningFuture;

impl ActorFuture for SerenityListeningFuture {
    type Output = ();
    type Actor = DiscordActor;

    fn poll(self: Pin<&mut Self>, srv: &mut Self::Actor, ctx: &mut _, task: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        // Get the internal discord client from the actor's state.
        let discord_client: &mut Client = srv.serenity_client
            // As &mut ref
            .as_mut()
            // Panic on None.
            .expect("Could not get discord client from actor.");

        discord_client.start_autosharded()
    }
}


/// Function add a name and info to an interaction used by serenity.
/// In this case builds the /whois command.
fn create_whois(interaction: &mut CreateInteraction) -> &mut CreateInteraction {
    // Create the argument object to this interaction
    let mut arg = CreateInteractionOption::default();
    arg
        .name("user")
        .description("The user to get information about.")
        .required(true)
        .kind(ApplicationCommandOptionType::User);

    // Add the command with the argument as "/whois".
    interaction.name("whois")
        .description("Get information about a user.")
        .add_interaction_option(arg)
}

/// Make the global serenity client to talk to discord.
/// Create all necessary interactions.
async fn init_serenity() -> Client {
    info!("Initializing Serenity Discord Client");

    // Get the Discord config
    let discord_conf: &DiscordConfig = &global_config().discord_config;

    // Log a link to invite the bot to a server.
    info!("Invite bot using \
        https://discord.com/api/oauth2/authorize?client_id={}&permissions=2147549184&response_type=code&scope=bot%20applications.commands",
          discord_conf.client_id.as_str());

    // Create the serenity client to talk to discord.
    return Client::builder(&discord_conf.bot_token)
        .raw_event_handler(Handler)
        .await
        .expect("Could not create serenity client");

    /*
    info!("Starting Serenity Discord Client");
    // start_autosharded blocks!!
    discord_client.start_autosharded()
        .await
        .expect("Could not start serenity client.");

    // Add the interactions.
    // Get reference to serenity's http client
    let http = &discord_client.cache_and_http.http;

    // Create the interaction on the global scope
    info!("Registering global Discord commands");
    let command = Interaction::create_global_application_command(http, application_id, create_whois)
        .await
        .expect("Could not create global application command.");

    debug!("Global Command Response:\n{:#?}", command);

    // Create the interaction for each of the debug guilds.
    for guild_id in discord_conf.debug_guild_ids.iter() {
        info!("Registering Discord commands for guild ID {}", guild_id);

        // Convert the guild ID
        let gid = GuildId::from(*guild_id);

        // Create the interaction on the guild.
        let command = Interaction::create_guild_application_command(http, gid, application_id, create_whois)
            .await
            .expect(format!("Could not create guild command for guild {}", guild_id).as_str());

        debug!("Guild ({}) command response:\n{:#?}", guild_id, command);
    }
     */
}

/// Zero-sized type representing an actix actor to talk to discord.

pub struct DiscordActor {
    thread: std::thread::JoinHandle<()>
}

impl Actor for DiscordActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // Initialize serenity client on start.

        // Make the client initialization future.
        let fut = Box::pin(init_serenity());
        // Wrap the future into an actix future.
        let actix_future = InitSerenityFuture {inner: fut};

        // Execute the future on this actor's context.
        ctx.wait(actix_future);

        // Wait for the client to initialize.
        let mut discord_client: Client = self.serenity_client
            .expect("Discord client has not initialized.");

        // Start listening for connections.
        info!("Listening for connections from Discord");
        ctx.spawn()
        discord_client.start_autosharded()
    }
}
