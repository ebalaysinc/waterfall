use std::net::Shutdown;
use std::sync::Arc;
use std::sync::mpsc;
use crate::IpParser;

use std::{
  io::{Read, Write},
  net::{TcpStream, SocketAddr},
  thread, time
};

pub fn socks5_proxy(proxy_client: &mut TcpStream, client_hook: impl Fn(&TcpStream, &[u8]) -> Vec<u8> + std::marker::Sync + std::marker::Send + 'static) {
  let mut client: TcpStream = proxy_client.try_clone().unwrap();

  let mut buffer = [0 as u8; 128];

  client.set_nodelay(true);
  client.read(&mut buffer);
  client.write_all(&[5, 0]);
  client.read(&mut buffer);
  
  let mut parsed_data: IpParser = IpParser::parse(Vec::from(buffer));
  let mut packet: Vec<u8> = vec![5, 0, 0, parsed_data.dest_addr_type];

  if parsed_data.dest_addr_type == 3 {
    packet.extend_from_slice(&[parsed_data.host_unprocessed.len().try_into().unwrap()]);
  }

  packet.extend_from_slice(&parsed_data.host_unprocessed.as_slice());
  packet.extend_from_slice(&parsed_data.port.to_be_bytes());

  // Create a socket connection and pipe to messages receiver 
  // Which is wrapped in other function

  let server_socket = TcpStream::connect(match parsed_data.host_raw.len() {
    4 => {
      let mut sl: [u8; 4] = [0, 0, 0, 0];

      for iter in 0..4 {
        sl[iter] = parsed_data.host_raw[iter];
      }

      SocketAddr::new(sl.into(), parsed_data.port)
    },
    16 => {
      let mut sl: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

      for iter in 0..16 {
        sl[iter] = parsed_data.host_raw[iter];
      }

      SocketAddr::new(sl.into(), parsed_data.port)
    },
    _ => SocketAddr::new([0, 0, 0, 0].into(), parsed_data.port)
  });

  match server_socket {
    Ok(mut socket) => {
      let _ = client.write_all(&packet);

      let _ = socket.set_nodelay(true);
      let _ = client.set_nodelay(true);

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
              } else { break }
            }, Err(_error) => break
          }

          thread::sleep(time::Duration::from_millis(30));
        }
      });

      thread::spawn(move || {
        let msg_buffer: &mut [u8] = &mut [0u8; 1024];
        let client_hook_fn = Arc::clone(&func);

        loop {
          match client1.read(msg_buffer) {
            Ok(size) => {
              if size > 0 {
                let _ = socket1.write_all(&client_hook_fn(&socket1, &msg_buffer[..size]));
              } else { break }
            }, Err(_error) => break
          }

          thread::sleep(time::Duration::from_millis(30));
        }
      });
    },
    Err(_) => { }
  }
}