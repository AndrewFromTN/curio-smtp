#[macro_use]
extern crate vec1;

use futures::future::Future;
use futures::prelude::*;
use futures::stream::Stream;
use getopt::prelude::*;
use new_tokio_smtp::error::GeneralError;
use new_tokio_smtp::send_mail::{EncodingRequirement, Mail, MailAddress, MailEnvelop};
use new_tokio_smtp::{command, Connection, ConnectionConfig, Domain};
use std::env;
use std::vec::Vec;
use tokio::sync::watch;

use curio_lib::consumer_service::ConsumerService;
use curio_lib::types::consumer::ConsumerTypes;
use curio_lib::types::messages::AuctionOutbid;
use curio_lib::types::messages::NotificationMessage;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    let mut opts = Parser::new(&args, "scupd:");

    let mut server_string: String = String::from("127.0.0.1:6789");
    let mut client_string: String = String::from("127.0.0.1:6785");
    let mut smtp_server: String = String::from("smtp.mydomain.io");
    let sender_address_str: String = String::from("noreply@mydomain.io");
    let mut username: String = String::from("");
    let mut password: String = String::from("");
    loop {
        match opts
            .next()
            .transpose()
            .expect("Failed to read command line argument.")
        {
            None => break,
            Some(opt) => match opt {
                Opt('s', Some(string)) => server_string = string,
                Opt('c', Some(string)) => client_string = string,
                Opt('u', Some(string)) => username = string,
                Opt('p', Some(string)) => password = string,
                Opt('d', Some(string)) => smtp_server = string,
                _ => unreachable!(),
            },
        }
    }

    let sender_address = MailAddress::from_unchecked(sender_address_str);
    let (tx, mut rx) = watch::channel(NotificationMessage::Init);
    tokio::spawn(async move {
        let subscriptions = vec![NotificationMessage::AuctionOutbid(AuctionOutbid::default())];

        let mut consumer_service = ConsumerService::new(
            server_string,
            client_string,
            ConsumerTypes::Email,
            subscriptions,
            tx,
        );

        consumer_service.establish_connection().await;
    });

    tokio::spawn(async move {
        let config = ConnectionConfig::builder(Domain::from_unchecked(smtp_server))
            .expect("resolving domain failed")
            .auth(
                command::auth::Plain::from_username(username, password)
                    .expect("username/password can not contain \\0 bytes"),
            )
            .use_start_tls()
            .build();

        while let Some(msg) = rx.recv().await {
            dbg!(msg.clone());

            let mut mails: Vec<Result<MailEnvelop, GeneralError>> = vec![];
            match msg {
                NotificationMessage::AuctionOutbid(ref outbid) => {
                    let send_to = MailAddress::from_unchecked(&outbid.outbidee_email);
                    let raw_mail = "pretend this is a mail message that has an html body loaded from template file.";
                    let mail_data = Mail::new(EncodingRequirement::None, raw_mail.to_owned());
                    let mail = MailEnvelop::new(sender_address.clone(), vec1![send_to], mail_data);

                    mails.push(Ok(mail));
                }
                NotificationMessage::AuctionStatus(ref status) => {
                    let send_to_seller = MailAddress::from_unchecked(&status.seller_email);
                    let send_to_buyer = MailAddress::from_unchecked(&status.buyer_email);

                    let raw_mail_buyer = "pretend this is a mail message that has an html body loaded from template file.";
                    let raw_mail_seller = "pretend this is a mail message that has an html body loaded from template file.";

                    let mail_data_buyer =
                        Mail::new(EncodingRequirement::None, raw_mail_buyer.to_owned());
                    let mail_data_seller =
                        Mail::new(EncodingRequirement::None, raw_mail_seller.to_owned());

                    let mail_buyer = MailEnvelop::new(
                        sender_address.clone(),
                        vec1![send_to_seller],
                        mail_data_seller,
                    );
                    let mail_seller = MailEnvelop::new(
                        sender_address.clone(),
                        vec1![send_to_buyer],
                        mail_data_buyer,
                    );

                    mails.push(Ok(mail_buyer));
                    mails.push(Ok(mail_seller));
                }
                _ => {}
            }

            let result = Connection::connect_send_quit(config.clone(), mails)
                .then(Result::Ok::<_, ()>)
                .for_each(|result| {
                    if let Err(err) = result {
                        eprintln!("[sending mail failed]: {}", err);
                    } else {
                        println!("[successfully sent mail]");
                    }
                    Ok(())
                })
                .wait();

            if let Err(e) = result {
                eprintln!("error sending: {:?}", e);
            }
        }
    });
}
