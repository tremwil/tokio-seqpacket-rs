#![cfg(any(target_os = "linux", target_os = "android"))]

use assert2::assert;
use std::io::{IoSliceMut, Read, Seek};
use tokio_seqpacket::{
	ancillary::{AncillaryMessageReader, OwnedAncillaryMessage},
	UnixSeqpacket,
};

mod ancillary_fd_helper;
use ancillary_fd_helper::receive_file_descriptor_socket;

#[tokio::test]
async fn peek_fd() {
	// Create a socket that receives a file descriptor
	let socket = receive_file_descriptor_socket(b"Content", b"Hello world!").await;

	let mut read_buf = [0u8; 64];
	let mut ancillary_buf = [0u8; 64];

	fn assert_cmsg(cmsg: AncillaryMessageReader<'_>) {
		// Check that we got exactly one control message containing file descriptors.
		let mut messages = cmsg.into_messages();
		assert!(let Some(OwnedAncillaryMessage::FileDescriptors(mut fds)) = messages.next());
		assert!(let None = messages.next());

		// Check that we got exactly one file descriptor in the first control message.
		assert!(let Some(fd) = fds.next());
		assert!(let None = fds.next());

		// Check that we can retrieve the message from the attached file.
		let mut file = std::fs::File::from(fd);
		let mut contents = Vec::new();
		assert!(let Ok(_) = file.read_to_end(&mut contents));
		assert!(contents == b"Content");

		// Seek back to the start of the file for the next run.
		assert!(let Ok(_) = file.rewind());
	}

	// We should be able to peek at the message (and thus get FDs) twice.
	for _ in 0..2 {
		assert!(let Ok((12, cmsg)) = socket.peek_vectored_with_ancillary(&mut [IoSliceMut::new(&mut read_buf)], &mut ancillary_buf).await);
		assert_cmsg(cmsg)
	}

	// We should be able to receive the message after peeking at it.
	assert!(let Ok((12, cmsg)) = socket.recv_vectored_with_ancillary(&mut [IoSliceMut::new(&mut read_buf)], &mut ancillary_buf).await);
	assert_cmsg(cmsg)
}

/// Test that receiving a message partially sets the data truncated flag.
#[tokio::test]
async fn recv_partial_truncated() {
	assert!(let Ok((a, b)) = UnixSeqpacket::pair());
	assert!(let Ok(12) = a.send(b"Hello world!").await);

	let mut buffer = [0u8; 5];
	assert!(let Ok((5, cmsg)) = b.recv_vectored_with_ancillary(&mut [IoSliceMut::new(&mut buffer)], &mut []).await);
	assert!(cmsg.is_data_truncated());
	assert_eq!(&buffer, b"Hello");
}

/// Test that peeking at a message with an insufficient buffer sets the data truncated flag.
#[tokio::test]
async fn peek_partial_truncated() {
	assert!(let Ok((a, b)) = UnixSeqpacket::pair());
	assert!(let Ok(12) = a.send(b"Hello world!").await);

	let mut buffer = [0u8; 128];
	assert!(let Ok((5, cmsg)) = b.peek_vectored_with_ancillary(&mut [IoSliceMut::new(&mut buffer[..5])], &mut []).await);
	assert!(cmsg.is_data_truncated());
	assert_eq!(&buffer[..5], b"Hello");

	assert!(let Ok((12, cmsg)) = b.recv_vectored_with_ancillary(&mut [IoSliceMut::new(&mut buffer)], &mut []).await);
	assert!(!cmsg.is_data_truncated());
	assert_eq!(&buffer[..12], b"Hello world!");
}
