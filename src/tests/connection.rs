// Copyright (c) 2015-2016 Ivo Wetzel

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
use std::net;
use std::iter;
use std::thread;
use std::time::Duration;

use super::mock::{create_connection, create_socket, MockOwner};
use super::super::{Connection, ConnectionState, Config, MessageKind, Handler};

#[test]
fn test_create() {
    let (conn, _, _) = create_connection(None);
    assert_eq!(conn.open(), true);
    assert_eq!(conn.congested(), false);
    assert!(conn.state() == ConnectionState::Connecting);
    assert_eq!(conn.rtt(), 0);
    assert_eq!(conn.packet_loss(), 0.0);
    let local_address: net::SocketAddr = "127.0.0.1:1234".parse().unwrap();
    let peer_address: net::SocketAddr = "255.1.1.2:5678".parse().unwrap();
    assert_eq!(conn.local_addr(), local_address);
    assert_eq!(conn.peer_addr(), peer_address);
}

#[test]
fn test_set_tick_rate() {
    let (mut conn, _, _) = create_connection(None);
    conn.set_config(Config {
        send_rate: 10,
        .. Config::default()
    });
}

#[test]
fn test_close_local() {

    let (mut conn, mut socket, mut socket_handle, mut owner, mut handler) = create_socket(None);
    let address = conn.peer_addr();

    // Initiate closure
    conn.close();
    assert_eq!(conn.open(), true);
    assert!(conn.state() == ConnectionState::Closing);

    // Connection should now be sending closing packets
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    socket_handle.assert_sent(vec![("255.1.1.2:5678", [
        // protocol id
        1, 2, 3, 4,

        // connection id
        (conn.id().0 >> 24) as u8,
        (conn.id().0 >> 16) as u8,
        (conn.id().0 >> 8) as u8,
         conn.id().0 as u8,

        0, 128, 85, 85, 85, 85  // closure packet data

    ].to_vec())]);

    // Connection should close once the drop threshold is exceeded
    thread::sleep(Duration::from_millis(90));
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    socket_handle.assert_sent_none();

    assert_eq!(conn.open(), false);
    assert!(conn.state() == ConnectionState::Closed);

}

#[test]
fn test_close_remote() {

    let (mut conn, mut owner, mut handler) = create_connection(None);
    assert!(conn.state() == ConnectionState::Connecting);

    // Receive initial packet
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        0, // local sequence number
        0, // remote sequence number we confirm
        0, 0, 0, 0 // bitfield

    ].to_vec(), 0, &mut owner, &mut handler);

    assert!(conn.state() == ConnectionState::Connected);

    // Receive closure packet
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        0, 128, 85, 85, 85, 85 // closure packet data

    ].to_vec(), 0, &mut owner, &mut handler);

    assert_eq!(conn.open(), false);
    assert!(conn.state() == ConnectionState::Closed);

}

#[test]
fn test_reset() {
    let (mut conn, _, _) = create_connection(None);
    conn.close();
    conn.reset();
    assert_eq!(conn.open(), true);
    assert!(conn.state() == ConnectionState::Connecting);
}

#[test]
fn test_send_sequence_wrap_around() {

    let (mut conn, mut socket, mut socket_handle, mut owner, mut handler) = create_socket(None);
    let address = conn.peer_addr();

    for i in 0..256 {

        conn.send_packet(&mut socket, &address, &mut owner, &mut handler);

        socket_handle.assert_sent(vec![("255.1.1.2:5678", [
            // protocol id
            1, 2, 3, 4,

            // connection id
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,

            i as u8, // local sequence number
            0, // remote sequence number
            0, 0, 0, 0  // ack bitfield

        ].to_vec())]);

    }

    // Should now wrap around
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    socket_handle.assert_sent(vec![("255.1.1.2:5678", [
        // protocol id
        1, 2, 3, 4,

        // connection id
        (conn.id().0 >> 24) as u8,
        (conn.id().0 >> 16) as u8,
        (conn.id().0 >> 8) as u8,
         conn.id().0 as u8,

        0, // local sequence number
        0, // remote sequence number
        0, 0, 0, 0  // ack bitfield

    ].to_vec())]);

}

#[test]
fn test_send_and_receive_packet() {

    let (mut conn, mut socket, mut socket_handle, mut owner, mut handler) = create_socket(None);
    let address = conn.peer_addr();

    // Test Initial Packet
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    socket_handle.assert_sent(vec![("255.1.1.2:5678", [
        // protocol id
        1, 2, 3, 4,

        // connection id
        (conn.id().0 >> 24) as u8,
        (conn.id().0 >> 16) as u8,
        (conn.id().0 >> 8) as u8,
         conn.id().0 as u8,

        0, // local sequence number
        0, // remote sequence number
        0, 0, 0, 0  // ack bitfield

    ].to_vec())]);

    // Test sending of written data
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    socket_handle.assert_sent(vec![("255.1.1.2:5678", [
        1, 2, 3, 4,
        (conn.id().0 >> 24) as u8,
        (conn.id().0 >> 16) as u8,
        (conn.id().0 >> 8) as u8,
         conn.id().0 as u8,
        1, // local sequence number
        0,
        0, 0, 0, 0

    ].to_vec())]);

    // Write buffer should get cleared
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    socket_handle.assert_sent(vec![("255.1.1.2:5678", [
        1, 2, 3, 4,
        (conn.id().0 >> 24) as u8,
        (conn.id().0 >> 16) as u8,
        (conn.id().0 >> 8) as u8,
         conn.id().0 as u8,

        2, // local sequence number
        0,
        0, 0, 0, 0

    ].to_vec())]);

    // Test receiving of a packet with acknowledgements for two older packets
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        17, // local sequence number
        2, // remote sequence number we confirm
        0, 0, 0, 3, // confirm the first two packets

    ].to_vec(), 0, &mut owner, &mut handler);

    // Receive additional packet
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        18, // local sequence number
        3, // remote sequence number we confirm
        0, 0, 0, 0

    ].to_vec(), 0, &mut owner, &mut handler);

    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        19, // local sequence number
        4, // remote sequence number we confirm
        0, 0, 0, 0

    ].to_vec(), 0, &mut owner, &mut handler);

    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0, // ConnectionID is ignored by receive_packet)
        27, // local sequence number
        4, // remote sequence number we confirm
        0, 0, 0, 0

    ].to_vec(), 0, &mut owner, &mut handler);

    // Test Receive Ack Bitfield
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    socket_handle.assert_sent(vec![("255.1.1.2:5678", [
        1, 2, 3, 4,
        (conn.id().0 >> 24) as u8,
        (conn.id().0 >> 16) as u8,
        (conn.id().0 >> 8) as u8,
         conn.id().0 as u8,

        3, // local sequence number
        27, // remove sequence number set by receive_packet)

        // Ack bitfield
        0, 0, 3, 128 // 0000_0000 0000_0000 0000_0011 1000_0000

    ].to_vec())]);

}

#[test]
fn test_send_and_receive_messages() {

    let (mut conn, mut socket, mut socket_handle, mut owner, mut handler) = create_socket(None);
    let address = conn.peer_addr();

    // Test Message Sending
    conn.send(MessageKind::Instant, b"Foo".to_vec());
    conn.send(MessageKind::Instant, b"Bar".to_vec());
    conn.send(MessageKind::Reliable, b"Test".to_vec());
    conn.send(MessageKind::Ordered, b"Hello".to_vec());
    conn.send(MessageKind::Ordered, b"World".to_vec());

    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    socket_handle.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            0,
            0,
            0, 0, 0, 0,

            // Foo
            0, 0, 0, 3, 70, 111, 111,

            // Bar
            0, 0, 0, 3, 66, 97, 114,

            // Test
            1, 0, 0, 4, 84, 101, 115, 116,

            // Hello
            2, 0, 0, 5, 72, 101, 108, 108, 111,

            // World
            2, 1, 0, 5, 87, 111, 114, 108, 100

        ].to_vec())
    ]);

    // Test Message Receiving
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        0,
        0,
        0, 0, 0, 0,

        // Foo
        0, 0, 0, 3, 70, 111, 111,

        // Bar
        0, 0, 0, 3, 66, 97, 114,

        // Test
        1, 0, 0, 4, 84, 101, 115, 116,

        // We actually test inverse receiving order here!

        // World
        2, 1, 0, 5, 87, 111, 114, 108, 100,

        // Hello
        2, 0, 0, 5, 72, 101, 108, 108, 111

    ].to_vec(), 0, &mut owner, &mut handler);

    // Get received messages
    let messages: Vec<Vec<u8>> = conn.received().collect();

    assert_eq!(messages, vec![
        b"Foo".to_vec(),
        b"Bar".to_vec(),
        b"Test".to_vec(),
        b"Hello".to_vec(),
        b"World".to_vec()
    ]);

    // Test Received dismissing
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        1,
        1,
        0, 0, 0, 0,

        // Foo
        0, 0, 0, 3, 70, 111, 111

    ].to_vec(), 0, &mut owner, &mut handler);

    // send_packet should dismiss any received messages which have not been fetched
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    socket_handle.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            1,
            1,
            0, 0, 0, 1

        ].to_vec())
    ]);

    let messages: Vec<Vec<u8>> = conn.received().collect();
    assert_eq!(messages.len(), 0);

}

#[test]
fn test_receive_invalid_packets() {

    let (mut conn, mut owner, mut handler) = create_connection(None);

    // Empty packet
    conn.receive_packet([].to_vec(), 0, &mut owner, &mut handler);

    // Garbage packet
    conn.receive_packet([
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14

    ].to_vec(), 0, &mut owner, &mut handler);

}

#[test]
fn test_rtt() {

    let (mut conn, mut socket, mut socket_handle, mut owner, mut handler) = create_socket(None);
    let address = conn.peer_addr();

    assert_eq!(conn.rtt(), 0);

    // First packet
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    socket_handle.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            0,
            0,
            0, 0, 0, 0
        ].to_vec())
    ]);

    thread::sleep(Duration::from_millis(500));
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        0,
        0, // confirm the packet above
        0, 0,
        0, 0

    ].to_vec(), 0, &mut owner, &mut handler);

    // Expect RTT value to have moved by 10% of the overall roundtrip time
    assert!(conn.rtt() >= 40);

    // Second packet
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    socket_handle.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            1,
            0,
            0, 0, 0, 0
        ].to_vec())
    ]);
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        1,
        1, // confirm the packet above
        0, 0,
        0, 0

    ].to_vec(), 0, &mut owner, &mut handler);

    // Third packet
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    socket_handle.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            2,
            1,
            0, 0, 0, 1
        ].to_vec())
    ]);
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        2,
        2, // confirm the packet above
        0, 0,
        0, 0

    ].to_vec(), 0, &mut owner, &mut handler);

    // Fourth packet
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    socket_handle.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            3,
            2,
            0, 0, 0, 3
        ].to_vec())
    ]);
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        3,
        3, // confirm the packet above
        0, 0,
        0, 0

    ].to_vec(), 0, &mut owner, &mut handler);

    // Fifth packet
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    socket_handle.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            4,
            3,
            0, 0, 0, 7
        ].to_vec())
    ]);
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        4,
        4, // confirm the packet above
        0, 0,
        0, 0

    ].to_vec(), 0, &mut owner, &mut handler);

    // Sixth packet
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    socket_handle.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            5,
            4,
            0, 0, 0, 15
        ].to_vec())
    ]);

    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        5,
        5, // confirm the packet above
        0, 0,
        0, 0

    ].to_vec(), 0, &mut owner, &mut handler);

    // Expect RTT to have reduced by 10%
    assert!(conn.rtt() <= 40);

}

#[test]
fn test_rtt_tick_correction() {

    let (mut conn, mut socket, mut socket_handle, mut owner, mut handler) = create_socket(None);
    let address = conn.peer_addr();

    assert_eq!(conn.rtt(), 0);

    // First packet
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    socket_handle.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            0,
            0,
            0, 0, 0, 0
        ].to_vec())
    ]);

    thread::sleep(Duration::from_millis(500));
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        0,
        0, // confirm the packet above
        0, 0,
        0, 0

    ].to_vec(), 500, &mut owner, &mut handler);

    // Expect RTT value to have been corrected by passed in tick delay
    assert!(conn.rtt() <= 10);

}

#[cfg(feature = "packet_handler_lost")]
#[test]
fn test_packet_loss() {

    struct PacketLossHandler {
        packet_lost_calls: u32,
        connection_calls: u32
    }

    impl Handler<MockOwner> for PacketLossHandler {

        fn connection_packet_lost(
            &mut self, _: &mut MockOwner, _: &mut Connection, packet: &[u8]
        ) {
            self.packet_lost_calls += 1;
            assert_eq!([
                0, 0, 0, 14, 80, 97, 99, 107, 101, 116, 32, 73, 110, 115, 116, 97, 110, 116,
                1, 0, 0, 15, 80, 97, 99, 107, 101, 116, 32, 82, 101, 108, 105, 97, 98, 108, 101,
                2, 0, 0, 14, 80, 97, 99, 107, 101, 116, 32, 79, 114, 100, 101, 114, 101, 100
            ].to_vec(), packet);
        }

        fn connection(&mut self, _: &mut MockOwner, conn: &mut Connection) {
            // Packet loss should have been reset
            assert_eq!(conn.packet_loss(), 0.0);
            self.connection_calls += 1;
        }

    }

    let config = Config {
        // set a low threshold for packet loss
        packet_drop_threshold: 10,
        .. Config::default()
    };

    let (mut conn, mut socket, mut socket_handle, mut owner, _) = create_socket(Some(config));
    let mut handler = PacketLossHandler {
        packet_lost_calls: 0,
        connection_calls: 0
    };
    let address = conn.peer_addr();

    conn.send(MessageKind::Instant, b"Packet Instant".to_vec());
    conn.send(MessageKind::Reliable, b"Packet Reliable".to_vec());
    conn.send(MessageKind::Ordered, b"Packet Ordered".to_vec());

    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    socket_handle.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            0,
            0,
            0, 0, 0, 0,

            // Packet 1
            0, 0, 0, 14, 80, 97, 99, 107, 101, 116, 32, 73, 110, 115, 116, 97, 110, 116,

            // Packet 2
            1, 0, 0, 15, 80, 97, 99, 107, 101, 116, 32, 82, 101, 108, 105, 97, 98, 108, 101,

            // Packet 3
            2, 0, 0, 14, 80, 97, 99, 107, 101, 116, 32, 79, 114, 100, 101, 114, 101, 100

        ].to_vec())
    ]);

    assert_eq!(conn.packet_loss(), 0.0);

    // Wait a bit so the packets will definitely get dropped
    thread::sleep(Duration::from_millis(20));

    // Now receive a packet and check for the lost packets
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        0, 2, 0, 0, // Set ack seq to non-0 so we trigger the packet loss
        0, 0

    ].to_vec(), 0, &mut owner, &mut handler);

    assert_eq!(handler.connection_calls, 1);

    // RTT should be left untouched the lost packet
    assert_eq!(conn.rtt(), 0);

    // But packet loss should spike up
    assert_eq!(conn.packet_loss(), 100.0);

    // Lost handler should have been called once
    assert_eq!(handler.packet_lost_calls, 1);

    // The messages from the lost packet should have been re-inserted into
    // the message_queue and should be send again with the next packet.
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    socket_handle.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            1,
            0,
            0, 0, 0, 0,

            // Packet 2
            1, 0, 0, 15, 80, 97, 99, 107, 101, 116, 32, 82, 101, 108, 105, 97, 98, 108, 101,

            // Packet 3
            2, 0, 0, 14, 80, 97, 99, 107, 101, 116, 32, 79, 114, 100, 101, 114, 101, 100

        ].to_vec())
    ]);

    // Fully receive the next packet
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        0, 1, 0, 0,
        0, 0

    ].to_vec(), 0, &mut owner, &mut handler);

    // Packet loss should now go down
    assert_eq!(conn.packet_loss(), 50.0);

}

#[cfg(feature = "packet_handler_compress")]
#[test]
fn test_packet_compression() {

    struct PacketCompressionHandler {
        packet_compress_calls: u32,
        packet_decompress_calls: u32
    }

    impl Handler<MockOwner> for PacketCompressionHandler {

        fn connection_packet_compress(
            &mut self, _: &mut MockOwner, conn: &mut Connection,
            packet: Vec<u8>, data: &[u8]
        ) -> Vec<u8> {

            self.packet_compress_calls += 1;

            // Packet should already contain header
            assert_eq!([
                1, 2, 3, 4,
                (conn.id().0 >> 24) as u8,
                (conn.id().0 >> 16) as u8,
                (conn.id().0 >> 8) as u8,
                 conn.id().0 as u8,
                0, 0,
                0, 0, 0, 0

            ].to_vec(), &packet[..]);

            // Expect actual packet data
            assert_eq!([
                0, 0, 0, 3, 70, 111, 111, // Foo
                0, 0, 0, 3, 66, 97, 114 // Bar
            ].to_vec(), &data[..]);

            // Return a empty compression result
            packet
        }

        fn connection_packet_decompress(
            &mut self, _: &mut MockOwner, _: &mut Connection, data: &[u8]
        ) -> Vec<u8> {

            // Packet data should be empty
            assert_eq!(data.len(), 0);

            self.packet_decompress_calls += 1;

            // Inject to messages
            [
                0, 0, 0, 3, 70, 111, 111, // Foo
                0, 0, 0, 3, 66, 97, 114 // Bar
            ].to_vec()

        }

    }

    let config = Config {
        // set a low threshold for packet loss
        packet_drop_threshold: 10,
        .. Config::default()
    };

    let (mut conn, mut socket, mut socket_handle, mut owner, _) = create_socket(Some(config));
    let mut handler = PacketCompressionHandler {
        packet_compress_calls: 0,
        packet_decompress_calls: 0
    };

    let address = conn.peer_addr();

    // First we send a packet to test compression
    conn.send(MessageKind::Instant, b"Foo".to_vec());
    conn.send(MessageKind::Instant, b"Bar".to_vec());
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    socket_handle.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            0,
            0,
            0, 0, 0, 0

            // Compression Handler will remove all packet data here

        ].to_vec())
    ]);

    // Compress handler should have been called once
    assert_eq!(handler.packet_compress_calls, 1);

    // Then receive a packet to test for decompression
    conn.receive_packet([
        1, 2, 3, 4,
        0, 0, 0, 0,
        0, 0, 0, 0,
        0, 0
        // Decompression handler will inject the packet data here

    ].to_vec(), 0, &mut owner, &mut handler);

    // Decompress handler should have been called once
    assert_eq!(handler.packet_decompress_calls, 1);

    // Get received messages
    let mut messages = Vec::new();
    for msg in conn.received() {
        messages.push(msg);
    }

    assert_eq!(messages, vec![
        b"Foo".to_vec(),
        b"Bar".to_vec(),
    ]);

}

#[cfg(feature = "packet_handler_compress")]
#[test]
fn test_packet_compression_inflated() {

    struct PacketCompressionHandler {
        packet_compress_calls: u32
    }

    impl Handler<MockOwner> for PacketCompressionHandler {

        fn connection_packet_compress(
            &mut self, _: &mut MockOwner, _: &mut Connection,
            mut packet: Vec<u8>, data: &[u8]
        ) -> Vec<u8> {

            self.packet_compress_calls += 1;

            // Expect actual packet data
            assert_eq!(data.len(), 0);

            let mut buffer: Vec<u8> = iter::repeat(74).take(16).collect();
            packet.append(&mut buffer);

            // Return a compression result that is bigger than the input
            packet
        }

    }

    let config = Config {
        // set a low threshold for packet loss
        packet_drop_threshold: 10,
        .. Config::default()
    };

    let (mut conn, mut socket, mut socket_handle, mut owner, _) = create_socket(Some(config));
    let mut handler = PacketCompressionHandler {
        packet_compress_calls: 0
    };

    let address = conn.peer_addr();

    // First we send a packet to test compression
    conn.send_packet(&mut socket, &address, &mut owner, &mut handler);
    socket_handle.assert_sent(vec![
        ("255.1.1.2:5678", [
            1, 2, 3, 4,
            (conn.id().0 >> 24) as u8,
            (conn.id().0 >> 16) as u8,
            (conn.id().0 >> 8) as u8,
             conn.id().0 as u8,
            0,
            0,
            0, 0, 0, 0,

            // Compression handler will insert additional packet data here
            74, 74, 74, 74, 74, 74, 74, 74, 74, 74, 74, 74, 74, 74, 74, 74

        ].to_vec())
    ]);

    // Compress handler should have been called once
    assert_eq!(handler.packet_compress_calls, 1);

}

