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
    debit: String,
    credit: String,
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
        r"友宝", // 0
        "滴滴",
        "三和", // 2
        "林俊通",
        "北京理工大学珠海学院", // 4
        "学生公寓店",
        "花呗", // 6
        "星火自选餐厅",
        "快餐", //8,
        "星之火",
        "中国铁路", //10
        "饿了么",
        "哈啰", //12,
        "公交",
        "公共交通", //14
        "珠海机场汽车运输有限公司",
        "便利店", //16,
        "北京通达无限",
        "天弘基金管理有限公司", //18
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
                    let counterpart = record[1].trim();
                    let orig_desc = record[3].trim();
                    let amount = record[5].trim();

                    let (debit, credit) = match record[0].trim() {
                        "支出" => ("", amount),
                        "收入" => (amount, ""),
                        _ => {
                            if orig_desc.contains("收益发放") {
                                (amount, "")
                            } else if orig_desc.contains("余额宝-自动转入") {
                                ("", amount)
                            } else if orig_desc.contains("余额宝-转出到余额") {
                                (amount, "")
                            } else if orig_desc.contains("充值-") {
                                ("", amount)
                            } else if orig_desc.contains("余额宝-单次转入") {
                                ("", amount)
                            } else if orig_desc.contains("蚂蚁借呗放款至余额") {
                                (amount, "")
                            } else if orig_desc.contains("蚂蚁借呗还款") {
                                ("", amount)
                            } else if orig_desc.contains("还款-花呗") {
                                ("", amount)
                            } else if orig_desc.contains("退款") {
                                (amount, "")
                            } else if orig_desc.contains("信用卡还款") {
                                ("", amount)
                            } else if orig_desc.contains("蚂蚁借呗放款至银行卡") {
                                (amount, "")
                            } else if orig_desc.contains("快速提现") {
                                ("", amount)
                            } else if orig_desc.contains("心愿单-定时收款") {
                                ("", amount)
                            } else if orig_desc.contains("钉钉转账-餐费") {
                                ("", amount)
                            } else if orig_desc.contains("转账到银行卡-转账") {
                                ("", amount)
                            } else if orig_desc.contains("转账收款到余额宝") {
                                ("", amount)
                            } else if orig_desc.contains("卖出至余额宝") {
                                (amount, "")
                            } else if orig_desc.contains("余额宝-转出到银行卡") {
                                (amount, "")
                            } else if orig_desc.contains("余额宝-蚂蚁星愿自动攒入") {
                                ("", amount)
                            } else {
                                unimplemented!("{}", orig_desc)
                            }
                        }
                    };

                    let matches = counterpart_set
                        .matches(counterpart)
                        .into_iter()
                        .collect::<Vec<_>>();
                    if matches.len() > 1 {
                        panic!("{} {}", counterpart, orig_desc);
                    }

                    let (description, counter_account) = if matches.is_empty() {
                        if orig_desc.contains("-收益发放") {
                            (orig_desc, "收入:利息")
                        } else if orig_desc.contains("相互宝分摊") {
                            (orig_desc, "支出:保险")
                        } else {
                            println!("{} {} 杂项", counterpart, orig_desc);
                            (orig_desc, "支出:杂项")
                        }
                    } else {
                        match matches[0] {
                            0 => (orig_desc, "支出:零食"),
                            1 => ("滴滴打车", "支出:公共交通:打车"),
                            2 => ("三和超市", "支出:食品杂货"),
                            3 => ("水电费", "支出:水电费"),
                            4 => {
                                if orig_desc.contains("电费充值") {
                                    ("电费", "支出:水电费:电")
                                } else if orig_desc.contains("校园一卡通") {
                                    ("饭卡", "支出:用餐")
                                } else if orig_desc.contains("校园网") {
                                    ("宽带", "支出:网络")
                                } else if orig_desc.contains("系统跳转") {
                                    ("住宿费", "支出:租金")
                                } else if orig_desc.contains("四六级") {
                                    ("四六级报名费", "支出:教育")
                                } else if orig_desc.contains("毕业生图像信息采集缴费") {
                                    ("2020届毕业生图像信息采集缴费", "支出:教育")
                                } else {
                                    panic!("{}", &orig_desc);
                                }
                            }
                            5 => ("零食", "支出:零食"),
                            6 => (orig_desc, "负债:花呗"),
                            7 => ("用餐-星火自选餐厅", "支出:用餐"),
                            8 | 9 | 11 => (orig_desc, "支出:用餐"),
                            10 | 12 | 13 | 14 => (orig_desc, "支出:公共交通"),
                            15 => ("机场大巴", "支出:公共交通"),
                            16 => ("便利店", "支出:食品杂货"),
                            17 => ("滴滴", "支出:出租车"),
                            18 => (orig_desc, "余额宝"),
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
                            debit: debit.into(),
                            credit: credit.into(),
                        },
                    );
                }
            }
            RecordType::Wechat => {
                // for record in reader.records() {
                //     let record = record?;
                //     if record[7].trim() != "支付成功"
                //         && record[7].trim() != "已存入零钱"
                //         && record[7].trim() != "朋友已收钱"
                //         && record[7].trim() != "已收钱"
                //         && record[7].trim() != "充值完成"
                //         && record[7].trim() != "已转账"
                //         && record[7].trim() != "已全额退款"
                //         && record[7].trim() != "提现已到账"
                //         && !record[7].trim().starts_with("已退款")
                //     {
                //         bail!("{}", record[7].trim());
                //     }
                //     let datetime =
                //         chrono::NaiveDateTime::parse_from_str(&record[0].trim(), "%F %T").unwrap();
                //     let amount = record[5].trim();
                //     let (debit, credit) = match record[4].trim() {
                //         "支出" | "/" => ("", amount),
                //         "收入" => (amount, ""),
                //         _ => {
                //             dbg!(&record[4]);
                //             unimplemented!("{:?} {:?}", path, reader.position());
                //         }
                //     };
                //     let counterpart = record[2].trim();
                //     let matches = counterpart_set
                //         .matches(counterpart)
                //         .into_iter()
                //         .collect::<Vec<_>>();
                //     if matches.len() > 1 {
                //         panic!("fuck");
                //     }
                //     let (description, counter_account) = if matches.is_empty() {
                //         if record[1].trim() == "转账" {
                //             (format!("转账给{}", record[2].trim()), "支出:杂项")
                //         } else if record[1].trim() == "微信红包" {
                //             (format!("{}的红包", record[2].trim()), "支出:礼品")
                //         } else {
                //             (record[3].trim().to_owned(), "支出:杂项")
                //         }
                //     } else {
                //         match matches[0] {
                //             0 => (record[3].trim().to_owned(), "支出:零食"),
                //             1 => ("滴滴打车".to_owned(), "支出:公共交通:打车"),
                //             2 => ("三和超市".to_owned(), "支出:食品杂货"),
                //             3 => ("水电费".to_owned(), "支出:水电费"),
                //             4 => (record[3].trim().to_owned(), "支出:饮料"),
                //             5 | 6 => ("零食".to_owned(), "支出:零食"),
                //             7 => (record[3].trim().to_owned(), "负债:花呗"),
                //             _ => unimplemented!(),
                //         }
                //     };

                //     let account = record[6].trim();

                //     let matches = account_set.matches(account).into_iter().collect::<Vec<_>>();
                //     if matches.len() > 1 {
                //         println!("more than one match {} {:?}", account, path);
                //         continue;
                //     }
                //     if matches.is_empty() {
                //         println!("wechat no match {}", account);
                //         continue;
                //     }
                //     let account = match matches[0] {
                //         0 => "负债:花呗",
                //         1 | 9 => "资产:流动资产:微信钱包",
                //         2 => "资产:流动资产:活期存款:农业银行2378",
                //         3 => "资产:流动资产:活期存款:建设银行",
                //         4 => "资产:流动资产:活期存款:招商银行",
                //         5 => "资产:流动资产:活期存款:工商银行",
                //         6 => "资产:流动资产:活期存款:邮政储蓄银行",
                //         7 => "负债:招商银行信用卡",
                //         8 => "资产:流动资产:活期存款:杭州银行",
                //         _ => unimplemented!("{:?} {:?}", &record, reader.position()),
                //     };
                //     records.insert(
                //         datetime,
                //         Record {
                //             description: description.into(),
                //             account: account.into(),
                //             counter_account: counter_account.into(),
                //             credit: credit.into(),
                //             debit: debit.into(),
                //         },
                //     );
                // }
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
