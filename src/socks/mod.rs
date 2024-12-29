use std::net::Shutdown;
use std::sync::Arc;
use crate::IpParser;

use std::{
  io::{Read, Write},
  net::TcpStream,
  thread
};

pub fn socks5_proxy(proxy_client: &mut TcpStream, client_hook: impl Fn(TcpStream, &[u8]) -> Vec<u8> + std::marker::Sync + std::marker::Send + 'static) {
  let mut client: TcpStream = match proxy_client.try_clone() {
    Ok(socket) => socket,
    Err(_error) => {
      println!("Connection dropped: failed to clone socket. {:?}", proxy_client);

      return;
    }
  };

  let _ = client.set_nodelay(true);

  let mut buffer = [0 as u8; 200];

  let mut state_auth: bool = false;

  while match client.read(&mut buffer) {
    Ok(_s) => {
      if !state_auth {
        // Client authentification packet. Reply with [5, 1] which stands for
        // no-authentification 

        match client.write(&[0x05, 0x00]) {
          Ok(_size) => println!("Authentification complete!"),
          Err(_error) => return
        }

        state_auth = true;
      } else {
        let mut parsed_data: IpParser = IpParser::parse(Vec::from(buffer));

        println!("Parsed IP data: {:?}", parsed_data);

        // Accept authentification and return connected IP
        // By default, if connected IP is not equal to the one
        // Client have chosen, the connection is dropped
        // So we can't just put [0, 0, 0, 0]

        // Server accept structure:
        // 0x05, 0, 0, dest_addr_type as u8, ..parsed_ip, port.as_bytes()

        let mut packet: Vec<u8> = vec![5, 0, 0, parsed_data.dest_addr_type];

        packet.extend_from_slice(&parsed_data.host_raw.as_slice());
        packet.extend_from_slice(&parsed_data.port.to_be_bytes());

        match client.write(&packet) {
          Ok(_size) => println!("[Auth] Accepted! {:?}", buffer),
          Err(_error) => return
        }

        // Create a socket connection and pipe to messages receiver 
        // Which is wrapped in other function

        let server_socket = TcpStream::connect(
          parsed_data.host_raw.
          iter_mut()
          .map(|fag| fag.to_string())
          .collect::<Vec<_>>()
          .join(".") + ":" + &parsed_data.port.to_string());

        println!("Socket instanced");

        match server_socket {
          Ok(mut socket) => {
            let _ = socket.set_nodelay(true);
            println!("Connected to socket: {:?}", socket);

            let mut socket1: TcpStream = socket.try_clone().unwrap();
            let mut client1: TcpStream = client.try_clone().unwrap();

            let func = Arc::new(client_hook);

            thread::spawn(move || {
              let msg_buffer: &mut [u8] = &mut [0u8; 1024];

              loop {
                match socket.read(msg_buffer) {
                  Ok(size) => {
                    if size > 0 {
                      let _ = client.write_all(&msg_buffer[..size]);
                    } else {
                      let _ = client.shutdown(Shutdown::Both);
                    }
                  }, Err(_error) => { }
                }
              }
            });

            thread::spawn(move || {
              let msg_buffer: &mut [u8] = &mut [0u8; 1024];

              loop {
                let client_hook_fn = Arc::clone(&func);

                match client1.read(msg_buffer) {
                  Ok(size) => {
                    if size > 0 {
                      let _ = socket1.write_all(&client_hook_fn(socket1.try_clone().unwrap(), &msg_buffer[..size]));
                    } else {
                      let _ = socket1.shutdown(Shutdown::Both);
                    }

                  }, Err(_error) => continue
                }
              }
            });

            return;
          },
          Err(_error) => {
            println!("Critical error happened! Couldn't restore from normal state, closing sockets.");
          }
        }
      }

      true
    },
    Err(_error) => false
  } {}

  println!("Connection complete: {:?}", client);
}