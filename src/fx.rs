use std::{collections::HashMap, io::Cursor, sync::Arc};

use anyhow::Result;
use polars::io::{
    csv::read::{CsvParseOptions, CsvReadOptions, NullValues},
    parquet::{read::ParquetReader, write::ParquetWriter},
    SerReader,
};
use polars_core::frame::DataFrame;
use reqwest::StatusCode;
use time::{Date, Month, OffsetDateTime, Time};

#[derive(Debug)]
pub struct HistoryCache {
    pub fx: HashMap<(String, String), Arc<Pair>>,
}

impl HistoryCache {
    pub fn new() -> Self {
        Self { fx: HashMap::new() }
    }

    pub fn load() -> Result<Self> {
        let mut fx = HashMap::new();
        std::fs::create_dir_all("portfolio/fx")?;
        for file in std::fs::read_dir("portfolio/fx")? {
            let file = file?;
            let path = file.path();
            let mut split = path.file_stem().unwrap().to_str().unwrap().split(':');
            let from: String = split.next().unwrap().into();
            let to: String = split.next().unwrap().into();
            let file = std::fs::File::open(path)?;
            let df = ParquetReader::new(file).finish().unwrap();
            fx.insert(
                (from.clone(), to.clone()),
                Arc::new(Pair {
                    from,
                    to,
                    dirty: false,
                    rates: df,
                }),
            );
        }
        Ok(Self { fx })
    }

    pub async fn get(&mut self, from: &str, to: &str) -> Result<Arc<Pair>> {
        let start = OffsetDateTime::new_utc(
            Date::from_calendar_date(2016, Month::January, 1)?,
            Time::from_hms_nano(0, 0, 0, 0)?,
        )
        .unix_timestamp();
        let end = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        if let Some(rate) = self.fx.get(&(from.into(), to.into())) {
            let x = rate
                .rates
                .get_column_index("Date")
                .and_then(|index| rate.rates.get(index))
                .and_then(|r| r.first().cloned())
                .and_then(|v| v.try_extract::<i32>().ok())
                .unwrap_or_default();
            let x = (3600 * 24) * x;
            if end < (x as u64) {
                println!("{end} < {x}");
                return Ok(rate.clone());
            }
        }
        let interval = "1d";

        let client = reqwest::Client::builder().user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36").build()?;
        let response = client
            .get(&format!(
                "https://query1.finance.yahoo.com/v7/finance/download/{from}{to}=X?period1={}&period2={}&interval={}&events=history&includeAdjustedClose=true",
                start,
                end,
                interval,
            ))
            .send()
            .await?;

        if response.status() != StatusCode::OK {
            panic!("{}", response.text().await?);
        };
        let text = response.text().await?;
        let rates = CsvReadOptions::default()
            .with_parse_options(
                CsvParseOptions::default()
                    .with_null_values(Some(NullValues::AllColumnsSingle("null".into())))
                    .with_try_parse_dates(true),
            )
            .into_reader_with_file_handle(Cursor::new(&text))
            .finish()?;

        let pair = Arc::new(Pair {
            from: from.into(),
            to: to.into(),
            dirty: true,
            rates,
        });

        self.fx.insert((from.into(), to.into()), pair.clone());
        self.save()?;

        Ok(pair)
    }

    pub fn save(&self) -> Result<()> {
        for entry in self.fx.values() {
            if entry.dirty {
                let mut file = std::fs::File::create(format!(
                    "portfolio/fx/{}:{}.parquet",
                    entry.from, entry.to
                ))?;
                ParquetWriter::new(&mut file).finish(&mut entry.rates.clone())?;
            }
        }
        Ok(())
    }
}

impl Default for HistoryCache {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct Pair {
    pub from: String,
    pub to: String,
    pub dirty: bool,
    pub rates: DataFrame,
}
