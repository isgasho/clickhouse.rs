use std::{
    io::{self, Write},
    mem,
    time::Instant,
};

use clickhouse::{Client, Compression, Reflection};
use serde::Deserialize;

#[derive(Debug, Reflection, Deserialize)]
struct InputRow<'a> {
    sequence_no: u64,
    trace_id: u64,
    endpoint_name: &'a str,
    message_type: &'a str,
    message: &'a str,
}

async fn stress(client: &Client, count: u64) {
    let sql = "
        SELECT ?fields
          FROM internal_message_log
         WHERE service_name = 'pretrader'
           AND node_no = 0
           AND launch_no = 4108
           AND sequence_no BETWEEN 1156299204327374856 AND 1156299204327374856+?
           AND endpoint_direction = 'Input'
    ";

    let start = Instant::now();

    let mut select_count = 0;
    let mut row_count = 0;
    let mut total_size = 0;
    let mut prev_speed = 0.;

    loop {
        let mut cursor = client
            .query(sql)
            .bind(count)
            .fetch::<InputRow<'_>>()
            .unwrap();

        while let Some(row) = cursor.next().await.unwrap() {
            row_count += 1;
            total_size += row.message.len()
                + row.message_type.len()
                + row.endpoint_name.len()
                + mem::size_of_val(&row);
        }

        let spent = start.elapsed().as_secs_f64();
        let speed = row_count as f64 / spent / 1000.;
        print!(
            "\r{:>2}M\t{:>6.1}s\t{:>6.1}MiB\t{:>6.1}MiB/s\t{:>6.1}K/s",
            count / 1_000_000,
            spent / select_count as f64,
            total_size as f64 / select_count as f64 / 1024. / 1024.,
            total_size as f64 / spent / 1024. / 1024.,
            speed,
        );
        io::stdout().flush().unwrap();

        if (speed - prev_speed).abs() / prev_speed < 0.03 || spent > 60. {
            break;
        }

        prev_speed = speed;
        select_count += 1;
    }

    println!();
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args();
    args.next();
    let compression = match args.next().as_deref().expect("compression is unspecified") {
        "none" => Compression::None,
        "lz4" => Compression::Lz4,
        //"gzip" => Compression::Gzip,
        //"zlib" => Compression::Zlib,
        //"brotli" => Compression::Brotli,
        _ => panic!("bad compression"),
    };
    let local = &args.next().expect("locality is unspecified") == "1";

    let client = if local {
        Client::default().with_url("http://localhost:8123")
    } else {
        Client::default()
            .with_url("http://ch-stg-imessage.zubr.tech:8123")
            .with_database("uat_core_imessage")
            .with_user("ppavelko")
            .with_password("wNGn7nQ59vARqAkLWJNnM3W8yCUCKuhy")
    };

    let client = client.with_compression(compression);

    println!("{:?}", compression);
    println!("count\t  spent\t  size   \t   thrpt   \t  speed");

    for count in &[1u64, 2, 5, 10] {
        stress(&client, count * 1_000_000).await;
    }
}
