use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConductorConfig {
    pub database_url: String,
}

impl ConductorConfig {
    pub fn from_env() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .context("DATABASE_URL must point at the conductor Postgres database")?;

        Ok(Self { database_url })
    }
}

#[cfg(test)]
mod tests {
    use super::ConductorConfig;

    #[test]
    fn reads_database_url_from_environment() {
        temp_env::with_var(
            "DATABASE_URL",
            Some("postgres://postgres:postgres@127.0.0.1:54322/postgres"),
            || {
                let config = ConductorConfig::from_env().expect("config should load");
                assert_eq!(
                    config.database_url,
                    "postgres://postgres:postgres@127.0.0.1:54322/postgres"
                );
            },
        );
    }

    #[test]
    fn errors_when_database_url_is_missing() {
        temp_env::with_var("DATABASE_URL", Option::<&str>::None, || {
            let error = ConductorConfig::from_env().expect_err("missing env should fail");
            assert!(error.to_string().contains("DATABASE_URL"));
        });
    }
}
