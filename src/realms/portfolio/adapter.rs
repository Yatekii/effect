use std::{
    collections::HashMap,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Ok, Result};
use axum::async_trait;

use crate::{
    banks::{load, LedgerRecord},
    handler::ledger::{create::CreateLedgerRequest, update::UpdateLedgerRequest},
    processing::process,
};

use super::state::{Account, Owner, Portfolio, SerdeAccount, SerdePortfolio};

#[async_trait]
pub trait Adapter: Send + Sync {
    fn load(&self, owner: Owner) -> Result<Portfolio>;
    fn store(&self, state: &Portfolio) -> Result<()>;
    async fn create_ledger(
        &self,
        portfolio: Portfolio,
        account: CreateLedgerRequest,
    ) -> Result<String>;
    async fn update_ledger(
        &self,
        portfolio: Portfolio,
        id: String,
        account: UpdateLedgerRequest,
    ) -> Result<String>;
    async fn delete_ledger(&self, portfolio: Portfolio, id: &str) -> Result<()>;
    fn list_files(&self, owner: &Owner) -> Result<HashMap<String, Vec<PathBuf>>>;
    fn load_file(&self, owner: &Owner, id: &str, path: &Path) -> Result<Vec<LedgerRecord>>;
    fn add_file(&self, owner: &Owner, id: &str, name: &str, content: Vec<u8>) -> Result<()>;
    fn update_file(&self, owner: &Owner, id: &str, name: &str, content: Vec<u8>) -> Result<()>;
    fn delete_file(&self, owner: &Owner, id: &str, name: &str) -> Result<()>;
}

pub struct Production {
    path: PathBuf,
}

impl Production {
    const PORTFOLIO_FILE_NAME: &'static str = "portfolio.yaml";
    const PORTFOLIO_LEDGER_DIR: &'static str = "ledgers";

    pub(crate) fn new(path: PathBuf) -> Production {
        Production { path }
    }
}

#[async_trait]
impl Adapter for Production {
    fn store(&self, portfolio: &Portfolio) -> Result<()> {
        let accounts = portfolio
            .accounts
            .iter()
            .map(|(id, ledger)| {
                (
                    id.clone(),
                    SerdeAccount {
                        id: ledger.id.clone(),
                        owner: ledger.owner.clone(),
                        name: ledger.name.clone(),
                        currency: ledger.currency,
                        format: ledger.format,
                        initial_balance: ledger.initial_balance,
                        initial_date: ledger.initial_date,
                        spending: ledger.spending,
                    },
                )
            })
            .collect::<HashMap<String, SerdeAccount>>();
        serde_yaml::to_writer(
            std::fs::File::create("portfolio/portfolio.yaml")?,
            &SerdePortfolio {
                base_currency: portfolio.base_currency,
                accounts,
                stocks: vec![],
            },
        )?;
        let path = self
            .path
            .join(Self::PORTFOLIO_LEDGER_DIR)
            .join(&portfolio.owner);
        std::fs::create_dir_all(&path)?;
        // TODO:
        // for (id, ledger) in &portfolio.accounts {
        //     // let mut file = std::fs::File::create(path.join(format!("{}.parquet", id)))?;
        //     // let mut df = ledger.ledgers.clone();
        //     // ParquetWriter::new(&mut file).finish(&mut df)?;
        // }
        Ok(())
    }

    fn load(&self, owner: Owner) -> Result<Portfolio> {
        let file = File::open(self.path.join(Self::PORTFOLIO_FILE_NAME))?;
        let portfolio: SerdePortfolio = serde_yaml::from_reader(file)?;

        let path = self.path.join(Self::PORTFOLIO_LEDGER_DIR).join(&owner);
        let accounts = portfolio
            .accounts
            .into_iter()
            .map(|(id, account)| {
                let path = path.join(account.id);
                let mut data = vec![];
                for entry in std::fs::read_dir(&path)
                    .with_context(|| anyhow!("could not open dir {}", path.display()))?
                {
                    let entry = entry?;
                    let path = entry.path();
                    data.extend(load(&id, path, account.format)?);
                }
                Ok((
                    id.clone(),
                    Account {
                        id,
                        owner: account.owner,
                        name: account.name,
                        currency: account.currency,
                        format: account.format,
                        records: process(data, account.initial_balance, account.initial_date)?,
                        initial_balance: account.initial_balance,
                        initial_date: account.initial_date,
                        spending: account.spending,
                    },
                ))
            })
            .collect::<Result<HashMap<_, _>>>()?;

        Ok(Portfolio {
            base_currency: portfolio.base_currency,
            stocks: vec![],
            accounts,
            owner,
        })
    }

    fn list_files(&self, owner: &Owner) -> Result<HashMap<String, Vec<PathBuf>>> {
        let file = File::open(self.path.join(Self::PORTFOLIO_FILE_NAME))?;
        let portfolio: SerdePortfolio = serde_yaml::from_reader(file)?;

        let path = self.path.join(Self::PORTFOLIO_LEDGER_DIR).join(owner);
        let lists = portfolio
            .accounts
            .into_iter()
            .map(|(id, account)| {
                let path = path.join(account.id);
                let files = std::fs::read_dir(&path)
                    .with_context(|| format!("could not read ledger directory {}", path.display()))?
                    .map(|dir_entry| {
                        let dir_entry = dir_entry.context("dir entry could not be read")?;

                        Ok(dir_entry.path().file_name().unwrap().into())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((id, files))
            })
            .collect::<Result<HashMap<_, _>>>()?;

        Ok(lists)
    }

    fn load_file(&self, owner: &Owner, id: &str, path: &Path) -> Result<Vec<LedgerRecord>> {
        let file = File::open(self.path.join(Self::PORTFOLIO_FILE_NAME))?;
        let portfolio: SerdePortfolio = serde_yaml::from_reader(file)?;

        let dir_path = self.path.join(Self::PORTFOLIO_LEDGER_DIR).join(owner);
        let ledger = portfolio
            .accounts
            .get(id)
            .with_context(|| format!("{id} does not exist in the ledgers"))?;
        load(id, dir_path.join(id).join(path), ledger.format)
    }

    fn add_file(&self, owner: &Owner, id: &str, name: &str, content: Vec<u8>) -> Result<()> {
        let dir_path = self.path.join(Self::PORTFOLIO_LEDGER_DIR).join(owner);
        let file_path = dir_path.join(id).join(name);
        let mut file = File::create(file_path)?;
        file.write_all(&content)?;
        Ok(())
    }

    fn update_file(&self, owner: &Owner, id: &str, name: &str, content: Vec<u8>) -> Result<()> {
        let dir_path = self.path.join(Self::PORTFOLIO_LEDGER_DIR).join(owner);
        let mut file = File::open(dir_path.join(id).join(name))?;
        file.write_all(&content)?;
        Ok(())
    }

    fn delete_file(&self, owner: &Owner, id: &str, name: &str) -> Result<()> {
        let dir_path = self.path.join(Self::PORTFOLIO_LEDGER_DIR).join(owner);
        std::fs::remove_file(dir_path.join(id).join(name))?;
        Ok(())
    }

    async fn create_ledger(
        &self,
        mut portfolio: Portfolio,
        account: CreateLedgerRequest,
    ) -> Result<String> {
        let id = slug::slugify(format!("{}-{}", &account.name, &account.currency));
        let CreateLedgerRequest {
            name,
            currency,
            format,
            initial_balance,
            initial_date,
            spending,
        } = account;

        let owner = portfolio.owner.clone();
        let dir_path = self
            .path
            .join(Self::PORTFOLIO_LEDGER_DIR)
            .join(&owner)
            .join(&id);
        std::fs::create_dir_all(&dir_path).with_context(|| {
            anyhow!(
                "Could not create ledger directory at `{}`",
                dir_path.display()
            )
        })?;

        portfolio.accounts.insert(
            id.clone(),
            Account {
                id: id.clone(),
                owner,
                name,
                currency,
                format,
                initial_balance,
                initial_date,
                spending,
                records: vec![],
            },
        );
        self.store(&portfolio)?;

        Ok(id)
    }

    async fn update_ledger(
        &self,
        mut portfolio: Portfolio,
        id: String,
        account: UpdateLedgerRequest,
    ) -> Result<String> {
        let new_id = slug::slugify(format!("{}-{}", &account.name, &account.currency));
        let owner = &portfolio.owner;
        let UpdateLedgerRequest {
            name,
            currency,
            format,
            initial_balance,
            initial_date,
            spending,
        } = account;

        let Some(account) = portfolio.accounts.get(&id) else {
            bail!("Ledger does not exist.");
        };

        if &account.owner != owner {
            bail!("Owner does not match!");
        }
        portfolio.accounts.insert(
            new_id.clone(),
            Account {
                id: new_id.clone(),
                owner: owner.clone(),
                name,
                currency,
                format,
                initial_balance,
                initial_date,
                spending,
                records: vec![],
            },
        );
        if new_id != id {
            let old_path = self
                .path
                .join(Self::PORTFOLIO_LEDGER_DIR)
                .join(owner)
                .join(&id);
            let new_path = self
                .path
                .join(Self::PORTFOLIO_LEDGER_DIR)
                .join(owner)
                .join(&new_id);
            std::fs::rename(old_path, new_path)?;
            portfolio.accounts.remove(&id);
        }
        self.store(&portfolio)?;

        Ok(new_id)
    }

    async fn delete_ledger(&self, mut portfolio: Portfolio, id: &str) -> Result<()> {
        let Some(account) = portfolio.accounts.get(id) else {
            bail!("Ledger does not exist.");
        };

        if account.owner != portfolio.owner {
            bail!("Owner does not match!");
        }

        portfolio.accounts.remove(id);
        self.store(&portfolio)?;

        let dir_path = self
            .path
            .join(Self::PORTFOLIO_LEDGER_DIR)
            .join(portfolio.owner)
            .join(id);
        std::fs::remove_dir(&dir_path).with_context(|| {
            anyhow!(
                "Could not create ledger directory at `{}`",
                dir_path.display()
            )
        })?;

        Ok(())
    }
}

pub struct Test;

#[async_trait]
impl Adapter for Test {
    fn load(&self, owner: Owner) -> Result<Portfolio> {
        Ok(Portfolio {
            base_currency: crate::fx::Currency::CHF,
            stocks: Default::default(),
            accounts: Default::default(),
            owner,
        })
    }

    fn store(&self, _state: &Portfolio) -> Result<()> {
        Ok(())
    }

    fn list_files(&self, _owner: &Owner) -> Result<HashMap<String, Vec<PathBuf>>> {
        Ok(Default::default())
    }

    fn load_file(&self, _owner: &Owner, _id: &str, _path: &Path) -> Result<Vec<LedgerRecord>> {
        Ok(Default::default())
    }

    fn add_file(&self, _owner: &Owner, _id: &str, _name: &str, _content: Vec<u8>) -> Result<()> {
        Ok(())
    }

    fn update_file(&self, _owner: &Owner, _id: &str, _name: &str, _content: Vec<u8>) -> Result<()> {
        Ok(())
    }

    fn delete_file(&self, _owner: &Owner, _id: &str, _name: &str) -> Result<()> {
        Ok(())
    }

    async fn create_ledger(
        &self,
        _portfolio: Portfolio,
        _account: CreateLedgerRequest,
    ) -> Result<String> {
        Ok(String::new())
    }

    async fn update_ledger(
        &self,
        _portfolio: Portfolio,
        _id: String,
        _account: UpdateLedgerRequest,
    ) -> Result<String> {
        Ok(String::new())
    }

    async fn delete_ledger(&self, _portfolio: Portfolio, _id: &str) -> Result<()> {
        Ok(())
    }
}
