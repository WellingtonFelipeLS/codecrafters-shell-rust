use std::{collections::BTreeMap, fs, io, mem, os::unix::fs::PermissionsExt, process};

pub enum OutputDirection {
    File(fs::File),
    PipeWriter(io::PipeWriter),
    Stdout(io::Stdout),
}

pub enum ErrDirection {
    File(fs::File),
    Stderr(io::Stderr),
}

impl io::Write for OutputDirection {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            OutputDirection::File(file) => file.write(buf),
            OutputDirection::PipeWriter(pipe_writer) => pipe_writer.write(buf),
            OutputDirection::Stdout(stdout) => stdout.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            OutputDirection::File(file) => file.flush(),
            OutputDirection::PipeWriter(pipe_writer) => pipe_writer.flush(),
            OutputDirection::Stdout(stdout) => stdout.flush(),
        }
    }
}

impl io::Write for ErrDirection {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            ErrDirection::File(file) => file.write(buf),
            ErrDirection::Stderr(stderr) => stderr.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            ErrDirection::File(file) => file.flush(),
            ErrDirection::Stderr(stderr) => stderr.flush(),
        }
    }
}

impl From<OutputDirection> for process::Stdio {
    fn from(value: OutputDirection) -> Self {
        match value {
            OutputDirection::File(file) => file.into(),
            OutputDirection::PipeWriter(pipe_writer) => pipe_writer.into(),
            OutputDirection::Stdout(stdout) => stdout.into(),
        }
    }
}

impl From<ErrDirection> for process::Stdio {
    fn from(value: ErrDirection) -> Self {
        match value {
            ErrDirection::File(file) => file.into(),
            ErrDirection::Stderr(stderr) => stderr.into(),
        }
    }
}

pub enum ProcessPosition {
    First,
    Middle,
    Last,
    FirstAndLast,
}

impl ProcessPosition {
    pub fn new(idx: usize, len: usize) -> Self {
        match idx {
            0 if idx == len - 1 => Self::FirstAndLast,
            0 => Self::First,
            _ if idx == len - 1 => Self::Last,
            _ => Self::Middle,
        }
    }

    pub fn is_first(&self) -> bool {
        matches!(self, Self::FirstAndLast | Self::First)
    }

    pub fn is_last(&self) -> bool {
        matches!(self, Self::FirstAndLast | Self::Last)
    }
}

pub fn is_executable(dir_entry: &fs::DirEntry) -> bool {
    if let Ok(metadata) = dir_entry.metadata()
        && metadata.permissions().mode() & 0o001 == 1
    {
        true
    } else {
        false
    }
}

pub fn read_user_input(buffer: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut open_single_quote = false;
    let mut open_double_quote = false;
    let mut escaped = false;
    let mut arg_buffer = String::new();

    for c in buffer.chars() {
        match c {
            '\\' => {
                if open_single_quote {
                    arg_buffer.push(c);
                } else if open_double_quote {
                    if escaped {
                        arg_buffer.push(c);
                        escaped = false;
                    } else {
                        escaped = true;
                    }
                } else if escaped {
                    arg_buffer.push(c);
                } else {
                    escaped = true;
                }
            }
            '\'' => {
                if open_double_quote {
                    if escaped {
                        arg_buffer.push('\\');
                        escaped = false;
                    }
                    arg_buffer.push(c);
                } else if escaped {
                    arg_buffer.push(c);
                    escaped = false;
                } else {
                    open_single_quote = !open_single_quote;
                }
            }
            '\"' => {
                if open_single_quote {
                    arg_buffer.push(c);
                } else if open_double_quote {
                    if escaped {
                        arg_buffer.push(c);
                        escaped = false;
                    } else {
                        open_double_quote = false;
                    }
                } else if escaped {
                    arg_buffer.push(c);
                    escaped = false;
                } else {
                    open_double_quote = !open_double_quote;
                }
            }
            x if x.is_whitespace() => {
                if open_single_quote {
                    arg_buffer.push(x);
                } else if open_double_quote {
                    if escaped {
                        arg_buffer.push('\\');
                        escaped = false;
                    }
                    arg_buffer.push(x);
                } else if escaped {
                    arg_buffer.push(x);
                    escaped = false;
                } else if !arg_buffer.is_empty() {
                    result.push(arg_buffer.clone());
                    arg_buffer.clear();
                }
            }
            x => {
                if open_double_quote && escaped {
                    arg_buffer.push('\\');
                }

                arg_buffer.push(x);
                escaped = false;
            }
        }
    }

    if !arg_buffer.is_empty() {
        result.push(arg_buffer);
    }

    result
}

pub fn verify_out_and_err_direction(
    user_inputs: &mut Vec<String>,
    pipe_writer: Option<io::PipeWriter>,
) -> io::Result<(OutputDirection, ErrDirection)> {
    let possible_file_name = user_inputs.pop();
    let possible_redirect_operator = user_inputs.pop();

    let output_direction = match pipe_writer {
        Some(pipe_writer) => OutputDirection::PipeWriter(pipe_writer),
        None => OutputDirection::Stdout(io::stdout()),
    };

    let result = match (
        possible_redirect_operator.as_deref(),
        possible_file_name.as_ref(),
    ) {
        (Some(">" | "1>"), Some(file_name)) => (
            OutputDirection::File(fs::File::create(file_name)?),
            ErrDirection::Stderr(io::stderr()),
        ),
        (Some(">>" | "1>>"), Some(file_name)) => (
            OutputDirection::File(
                fs::File::options()
                    .append(true)
                    .create(true)
                    .open(file_name)?,
            ),
            ErrDirection::Stderr(io::stderr()),
        ),
        (Some("2>"), Some(file_name)) => (
            output_direction,
            ErrDirection::File(fs::File::create(file_name)?),
        ),
        (Some("2>>"), Some(file_name)) => (
            output_direction,
            ErrDirection::File(
                fs::File::options()
                    .append(true)
                    .create(true)
                    .open(file_name)?,
            ),
        ),
        _ => {
            if let Some(x) = possible_redirect_operator {
                user_inputs.push(x);
            }

            if let Some(x) = possible_file_name {
                user_inputs.push(x);
            }

            (output_direction, ErrDirection::Stderr(io::stderr()))
        }
    };

    Ok(result)
}

pub struct BackGroundJobs {
    jobs: BTreeMap<usize, (String, process::Child)>,
    next_job_id: usize,
}

impl BackGroundJobs {
    pub fn new() -> Self {
        Self {
            jobs: BTreeMap::new(),
            next_job_id: 1,
        }
    }

    pub fn append(&mut self, input: Vec<String>) {
        if let Some((command, args)) = input.split_first()
            && let Some(child) = process::Command::new(command).args(args).spawn().ok()
        {
            println!("[{}] {}", self.next_job_id, child.id());

            self.jobs.insert(self.next_job_id, (input.join(" "), child));
            self.next_job_id += 1;
        }
    }

    pub fn list(&mut self, writer: &mut impl io::Write) -> io::Result<()> {
        let mut iter = mem::take(&mut self.jobs).into_iter();

        let most_recent_job = iter.next_back();
        let second_most_recent_job = iter.next_back();

        iter.try_for_each(|(job_id, (input, child))| {
            self.print_and_reap(job_id, input, child, writer, ' ', true)
        })?;

        if let Some((job_id, (input, child))) = second_most_recent_job {
            self.print_and_reap(job_id, input, child, writer, '-', true)?;
        };

        if let Some((job_id, (input, child))) = most_recent_job {
            self.print_and_reap(job_id, input, child, writer, '+', true)?;
        }

        Ok(())
    }

    fn print_and_reap(
        &mut self,
        job_id: usize,
        input: String,
        mut child: process::Child,
        writer: &mut impl io::Write,
        marker: char,
        print_running_processes: bool,
    ) -> io::Result<()> {
        match child.try_wait() {
            Ok(Some(_)) => writeln!(writer, "[{job_id}]{marker}  Done{:17}{input}", " "),
            Ok(None) => {
                if print_running_processes {
                    writeln!(writer, "[{job_id}]{marker}  Running{:17}{input} &", " ")?;
                }
                self.jobs.insert(job_id, (input, child));

                Ok(())
            }
            Err(_) => todo!(),
        }
    }

    pub fn check_jobs(&mut self) -> io::Result<()> {
        let mut iter = mem::take(&mut self.jobs).into_iter();

        let most_recent_job = iter.next_back();
        let second_most_recent_job = iter.next_back();

        iter.try_for_each(|(job_id, (input, child))| {
            self.print_and_reap(job_id, input, child, &mut io::stdout(), ' ', false)
        })?;

        if let Some((job_id, (input, child))) = second_most_recent_job {
            self.print_and_reap(job_id, input, child, &mut io::stdout(), '-', false)?;
        };

        if let Some((job_id, (input, child))) = most_recent_job {
            self.print_and_reap(job_id, input, child, &mut io::stdout(), '+', false)?;
        }

        Ok(())
    }
}
