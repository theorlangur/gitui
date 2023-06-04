use std::sync::mpsc;

use crate::AsyncAppNotification;

pub type BoxFeedback = Box<dyn AsyncJobFeedback + Send + Sync>;
pub type BoxJob = Box<dyn AsyncDynJob + Send + Sync>;
pub type JobFeedbackSender = mpsc::Sender<BoxFeedback>;
pub type JobFeedbackReceiver = mpsc::Receiver<BoxFeedback>;
pub type JobReceiver = mpsc::Receiver<BoxJob>;
pub type JobSender = mpsc::Sender<BoxJob>;
pub trait AsyncDynJob {
	fn run(
		&mut self,
		sender: JobFeedbackSender,
	) -> Option<BoxFeedback>;
	fn should_stop(&self) -> bool;
}

pub trait AsyncJobFeedback {
	fn visit(&mut self, app: &mut crate::app::App);
}

pub struct AsyncStopJob {}
impl AsyncDynJob for AsyncStopJob {
	fn run(
		&mut self,
		_sender: JobFeedbackSender,
	) -> Option<BoxFeedback> {
		None
	}
	fn should_stop(&self) -> bool {
		true
	}
}

pub struct AsyncJobList {}

impl AsyncJobList {
	pub fn new(
		tx_app: crossbeam_channel::Sender<AsyncAppNotification>,
	) -> (std::thread::JoinHandle<()>, JobSender, JobFeedbackReceiver)
	{
		let mut l = Self {};
		let (send_job, receive_job) = mpsc::channel();
		let (send_job_feeback, receive_job_feedback) =
			mpsc::channel();
		let t = std::thread::spawn(move || {
			l.run_loop(tx_app, send_job_feeback, receive_job);
		});
		(t, send_job, receive_job_feedback)
	}
	pub fn run_loop(
		&mut self,
		tx_app: crossbeam_channel::Sender<AsyncAppNotification>,
		sender: JobFeedbackSender,
		receiver: JobReceiver,
	) {
		loop {
			if let Ok(mut j) = receiver.recv() {
				let j = j.as_mut();
				if let Some(r) = j.run(sender.clone()) {
					if let Err(_) = sender.send(r) {
						break;
					}
				}
				if let Err(_) =
					tx_app.send(AsyncAppNotification::Notify)
				{
					break;
				}
				if j.should_stop() {
					break;
				}
			} else {
				//stop
				break;
			}
		}
	}
}
