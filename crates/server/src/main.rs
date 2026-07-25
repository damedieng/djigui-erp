//! Binaire serveur autonome (mode serveur sans coquille desktop — utile en
//! déploiement « poste serveur dédié » et pour les tests).

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,djigui_server=debug,djigui_core=debug".into()),
        )
        .init();

    djigui_server::serve(djigui_server::Config::from_env()).await
}
