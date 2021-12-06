use std::{path::PathBuf, str::FromStr};

use anyhow::{bail, Result};
use chrono::NaiveDateTime;
use clap::{App, Arg};
use multimap::MultiMap;
use regex::{Regex, RegexSet};
use serde::Deserialize;

enum RecordType {
    Alipay,
    Wechat,
}

struct RecordFile {
    ty: RecordType,
}

struct Record {
    description: String,
    account: String,
    counter_account: String,
    credit: String,
    debit: String,
}

fn main() -> Result<()> {
    let matches = App::new("transaction process")
        .arg(
            Arg::new("start_date")
                .long("start")
                .display_order(1)
                .default_value("1970-1-1"),
        )
        .arg(
            Arg::new("end_date")
                .long("end")
                .display_order(2)
                .default_value(&chrono::Local::today().format("%F").to_string()),
        )
        .arg(
            Arg::new("files")
                .short('f')
                .long("file")
                .required(true)
                .takes_value(true)
                .multiple_occurrences(true),
        )
        .get_matches();

    let files = matches.values_of("files").unwrap();

    let mut records: MultiMap<NaiveDateTime, Record> = MultiMap::new();

    for path in files {
        let mut reader = csv::Reader::from_path(path).expect(&format!("cannot read file {}", path));
        let ty = if path.contains("alipay") {
            RecordType::Alipay
        } else if path.contains("微信") {
            RecordType::Wechat
        } else {
            bail!("{}: unsupported record", path);
        };

        match ty {
            RecordType::Alipay => {
                for record in reader.records() {
                    let record = record?;
                    if record[6].trim() == "交易关闭" {
                        continue;
                    }
                    let datetime =
                        chrono::NaiveDateTime::parse_from_str(&record[10].trim(), "%F %T").unwrap();
                    let amount = record[5].trim();
                    let (debit, credit) = match record[0].trim() {
                        "支出" | "其他" => ("", amount),
                        "收入" => (amount, ""),
                        _ => {
                            dbg!(&record[0]);
                            unimplemented!()
                        }
                    };
                    let set = RegexSet::new(&[
                        r"友宝",
                        "滴滴",
                        "三和",
                        "林俊通",
                        "北京理工大学珠海学院",
                        "学生公寓店",
                        "全家",
                        "花呗",
                    ])
                    .unwrap();
                    let counterpart = record[1].trim();
                    let matches = set.matches(counterpart).into_iter().collect::<Vec<_>>();
                    if matches.len() > 1 {
                        panic!("fuck");
                    }
                    let (description, counter_account) = if matches.is_empty() {
                        (record[3].trim(), "支出:杂项")
                    } else {
                        match matches[0] {
                            0 => (record[3].trim(), "支出:零食"),
                            1 => ("滴滴打车", "支出:公共交通:打车"),
                            2 => ("三和超市", "支出:食品杂货"),
                            3 => ("水电费", "支出:水电费"),
                            4 => (record[3].trim(), "支出:饮料"),
                            5 | 6 => ("零食", "支出:零食"),
                            7 => (record[3].trim(), "负债:花呗"),
                            _ => unimplemented!(),
                        }
                    };

                    let set = RegexSet::new(&["花呗", "余额", "中国农业银行储蓄卡"]).unwrap();
                    let matches = set
                        .matches(record[4].trim())
                        .into_iter()
                        .collect::<Vec<_>>();
                    if matches.len() > 1 {
                        println!("{}", record[4].trim());
                        continue;
                    }
                    if matches.is_empty() {
                        println!("{}", record[4].trim());
                        continue;
                    }
                    let account = match matches[0] {
                        0 => "负债:花呗",
                        1 => "资产:流动资产:支付宝钱包",
                        2 => "资产:流动资产:活期存款:农业银行2378",
                        _ => unimplemented!("{:?} {:?}", &record, reader.position()),
                    };
                    println!("{}", description);
                    records.insert(
                        datetime,
                        Record {
                            description: description.into(),
                            account: account.into(),
                            counter_account: counter_account.into(),
                            credit: credit.into(),
                            debit: debit.into(),
                        },
                    );
                }
            }
            RecordType::Wechat => todo!(),
        }
    }
    let mut wtr = csv::Writer::from_path("foo.csv")?;
    for (datetime, records) in records {
        for rec in records {
            wtr.write_record(&[
                datetime.format("%F %T").to_string(),
                rec.description,
                rec.account,
                rec.counter_account,
                rec.debit,
                rec.credit,
            ])
            .unwrap();
        }
    }
    Ok(())
}
