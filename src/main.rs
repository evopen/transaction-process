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
                .required_unless_present("dir")
                .takes_value(true)
                .multiple_occurrences(true),
        )
        .arg(
            Arg::new("dir")
                .short('d')
                .long("dir")
                .required_unless_present("files")
                .takes_value(true)
                .multiple_occurrences(true),
        )
        .get_matches();

    let files: Vec<_> = if let Some(files) = matches.values_of("files") {
        files
            .into_iter()
            .map(|s| PathBuf::from_str(s).unwrap())
            .collect()
    } else if let Some(dir) = matches.value_of("dir") {
        std::fs::read_dir(dir)
            .unwrap()
            .into_iter()
            .map(|p| p.unwrap())
            .filter(|e| e.path().extension().map(|s| s.to_str().unwrap()) == Some("csv"))
            .map(|e| e.path().to_owned())
            .collect()
    } else {
        panic!()
    };

    let mut records: MultiMap<NaiveDateTime, Record> = MultiMap::new();

    let counterpart_set = RegexSet::new(&[
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

    let account_set = RegexSet::new(&[
        "花呗",
        "余额",
        "农业银行",
        "中国建设银行",
        "招商银行储蓄卡",
        "中国工商银行储蓄卡",
        "邮政储蓄银行",
        "招商银行信用卡",
        "杭州银行",
        "零钱",
    ])
    .unwrap();

    for path in files {
        let mut reader = csv::Reader::from_path(&path)
            .expect(&format!("cannot read file {}", path.to_str().unwrap()));
        let ty = if path.to_str().unwrap().contains("alipay") {
            RecordType::Alipay
        } else if path.to_str().unwrap().contains("微信") {
            RecordType::Wechat
        } else {
            bail!("{}: unsupported record", path.to_str().unwrap());
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

                    let counterpart = record[1].trim();
                    let matches = counterpart_set
                        .matches(counterpart)
                        .into_iter()
                        .collect::<Vec<_>>();
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

                    let account = record[4].trim();

                    let matches = account_set.matches(account).into_iter().collect::<Vec<_>>();
                    if matches.len() > 1 {
                        println!("more than one match {} {:?}", account, path);
                        continue;
                    }
                    if matches.is_empty() {
                        println!("no match {}", account);
                        continue;
                    }
                    let account = match matches[0] {
                        0 => "负债:花呗",
                        1 => "资产:流动资产:支付宝钱包",
                        2 => "资产:流动资产:活期存款:农业银行2378",
                        3 => "资产:流动资产:活期存款:建设银行",
                        4 => "资产:流动资产:活期存款:招商银行",
                        5 => "资产:流动资产:活期存款:工商银行",
                        6 => "资产:流动资产:活期存款:邮政储蓄银行",
                        7 => "负债:招商银行信用卡",
                        8 => "资产:流动资产:活期存款:杭州银行",
                        _ => unimplemented!("{:?} {:?}", &record, reader.position()),
                    };
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
            RecordType::Wechat => {
                for record in reader.records() {
                    let record = record?;
                    if record[7].trim() != "支付成功"
                        && record[7].trim() != "已存入零钱"
                        && record[7].trim() != "朋友已收钱"
                        && record[7].trim() != "已收钱"
                        && record[7].trim() != "充值完成"
                        && record[7].trim() != "已转账"
                        && record[7].trim() != "已全额退款"
                        && record[7].trim() != "提现已到账"
                        && !record[7].trim().starts_with("已退款")
                    {
                        bail!("{}", record[7].trim());
                    }
                    let datetime =
                        chrono::NaiveDateTime::parse_from_str(&record[0].trim(), "%F %T").unwrap();
                    let amount = record[5].trim();
                    let (debit, credit) = match record[4].trim() {
                        "支出" | "/" => ("", amount),
                        "收入" => (amount, ""),
                        _ => {
                            dbg!(&record[4]);
                            unimplemented!("{:?} {:?}", path, reader.position());
                        }
                    };
                    let counterpart = record[2].trim();
                    let matches = counterpart_set
                        .matches(counterpart)
                        .into_iter()
                        .collect::<Vec<_>>();
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

                    let account = record[6].trim();

                    let matches = account_set.matches(account).into_iter().collect::<Vec<_>>();
                    if matches.len() > 1 {
                        println!("more than one match {} {:?}", account, path);
                        continue;
                    }
                    if matches.is_empty() {
                        println!("wechat no match {}", account);
                        continue;
                    }
                    let account = match matches[0] {
                        0 => "负债:花呗",
                        1 | 9 => "资产:流动资产:微信钱包",
                        2 => "资产:流动资产:活期存款:农业银行2378",
                        3 => "资产:流动资产:活期存款:建设银行",
                        4 => "资产:流动资产:活期存款:招商银行",
                        5 => "资产:流动资产:活期存款:工商银行",
                        6 => "资产:流动资产:活期存款:邮政储蓄银行",
                        7 => "负债:招商银行信用卡",
                        8 => "资产:流动资产:活期存款:杭州银行",
                        _ => unimplemented!("{:?} {:?}", &record, reader.position()),
                    };
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
