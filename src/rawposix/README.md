# RawPOSIX [![Build Status](https://github.com/Lind-Project/safeposix-rust/actions/workflows/lind-selfhost.yml/badge.svg?branch=develop)](https://github.com/Lind-Project/safeposix-rust/actions/workflows/lind-selfhost.yml)

## Contents

* [Home](https://github.com/Lind-Project/lind-docs/blob/main/docs/RustPOSIX/Home.md)
* [Architecture](https://github.com/Lind-Project/lind-docs/blob/main/docs/RustPOSIX/Architecture.md)
* [Interface](https://github.com/Lind-Project/lind-docs/blob/main/docs/RustPOSIX/Interface.md)
* [Run Independently](https://github.com/Lind-Project/lind-docs/blob/main/docs/RustPOSIX/Run-Independently.md)
* [Security Model](https://github.com/Lind-Project/lind-docs/blob/main/docs/RustPOSIX/Security-Model.md)
* [Style Guide](https://github.com/Lind-Project/lind-docs/blob/main/docs/RustPOSIX/Style-Guide.md)
* [Testing and Debugging](https://github.com/Lind-Project/lind-docs/blob/main/docs/RustPOSIX/Testing-and-Debugging.md)

## Overview
This document provides a step-by-step guide to access the NYU SSH server and run Docker commands to set up and test the Lind project. If you face issues while accessing the server, troubleshooting steps are included to help you resolve them efficiently.

## Accessing the SSH Server
### SSH Command Format
To gain access, use the following SSH command format:

```bash
[username]@lind-server.engineering.nyu.edu
```

**Description**: Replace `[username]` with your NYU username to connect to the Lind server. This command will initiate a secure shell connection to the server, allowing you to work on the remote system.

### Troubleshooting Access Issues
#### Permission Denied
- **Description**: This usually means that the password is incorrect. Please try to recall the correct password or contact seniors for assistance, or ask for help in the Slack channel.

  ![Permission Denied](assets/permission-denied.readme.png)

#### Operation Timed Out / Unable to Resolve Host
- **Description**: This error generally means that your network is incorrect or unavailable.

  ![Operation Timed Out](assets/timed-out.readme.png)

### Network Verification
To verify network connectivity, follow these steps:

1. **Are you on an on-campus network?**

   ![On-campus Network](assets/network.readme.png)

2. If connected but still unable to access, contact seniors or use the Slack channel for support.

3. If not connected to the on-campus network, connect to VPN via the [NYU VPN Guide](https://www.nyu.edu/life/information-technology/infrastructure/network-services/vpn.html).

## Running Docker
### Running the Docker Container
![Running Docker](assets/docker.readme.png)

Once you have SSH access, run the Docker container with the following command:

```bash
docker run --privileged --ipc=host --cap-add=SYS_PTRACE -it securesystemslab/lind /bin/bash
```

**Description**: This command starts a Docker container using the image `securesystemslab/lind`. The options used are:

- `--privileged`: Grants extended privileges to the container.
- `--ipc=host`: Allows the container to share the host’s IPC namespace, enabling shared memory.
- `--cap-add=SYS_PTRACE`: Adds the capability to use `ptrace`, which is helpful for debugging.
- `-it`: Opens an interactive terminal session.
- `/bin/bash`: Launches a Bash shell inside the container.

**Note**: This command will give you an interactive shell inside the Docker container where you can run other commands.

## Next Steps After Running Docker
### Checking Git Branch and Updating
Once inside the container:

1. **Ensure you are on the `develop` branch**. Run the following commands to check and update:

    ```bash
    git branch
    ```

    **Description**: Displays the current branch. Ensure that you are on the `develop` branch.

    ```bash
    git pull
    ```

    **Description**: Fetches the latest updates from the remote repository and merges them into your current branch.

### Building Lind
1. **Update Contents**:
   - Run the following command to update contents to the newest version:

     ```bash
     make -2
     ```

     **Description**: This command will ensure that all the components are updated to the latest version. The `make` command runs the instructions defined in the Makefile, and the `-2` argument here specifies a particular target or set of actions.

2. **Build the Regular Lind Version**:
   - Run the following command to build the standard version of Lind:

     ```bash
     make -1
     ```

     **Description**: This command builds the standard version of Lind, preparing it for use.

## Running RawPOSIX
### Environment Setup for RawPOSIX
To run RawPOSIX, follow these steps:

1. **Navigate to the project directory and set up the environment**:

    ```bash
    cd src
    sudo rm -rf safeposix-rust
    git clone https://github.com/Lind-Project/RawPOSIX.git
    mv RawPOSIX/ safeposix-rust
    cd /home/lind/lind_project
    make -1
    ```

    **Description**:
    - `cd src`: Change to the source directory.
    - `sudo rm -rf safeposix-rust`: Remove the existing `safeposix-rust` directory (requires admin privileges).
    - `git clone ...`: Clone the RawPOSIX repository from GitHub.
    - `mv RawPOSIX/ safeposix-rust`: Rename the cloned directory to `safeposix-rust`.
    - `cd /home/lind/lind_project`: Change to the Lind project directory.
    - `make -1`: Build the project.

### Generating Network Devices for RawPOSIX
2. **Generate network devices** required for RawPOSIX:

    ```bash
    cd src/safeposix-rust
    ./gen_netdev.sh
    ```

    **Description**:
    - `cd src/safeposix-rust`: Change to the `safeposix-rust` directory.
    - `./gen_netdev.sh`: Run the script to generate network devices.

## Testing Suites
### Running Lind Test Suites
Navigate to the project root and run the following command:

```bash
cd /home/lind/lind_project
make test
```

**Description**: This command runs the full test suite for Lind, verifying that all components are functioning as expected.

### Running RawPOSIX Test Suites
To build and run the tests for RawPOSIX:

```bash
cd src/safeposix-rust
cargo build
cargo test
```

**Description**:
- `cargo build`: Compiles the Rust code for the RawPOSIX project.
- `cargo test`: Runs the test suite for RawPOSIX to verify functionality.

#### Running Specific Test Cases
To run a specific test case:

```bash
cargo test <TEST_CASE_NAME>
```

**Example**:

```bash
cargo test ut_lind_fs_mkdir_invalid_modebits
```

**Description**: This command runs a specific test case, allowing you to focus on one feature or functionality at a time.

## FAQ
### Handling Errors
1. **New error that requires a big fix**:
   - Contact the team and inform the seniors.
   - Open a GitHub issue to track the problem.

2. **Encountering a smaller issue**:
   - Check if an existing issue is logged. If not, create one at: [https://github.com/Lind-Project/RawPOSIX/issues/15](https://github.com/Lind-Project/RawPOSIX/issues/15).

### Tagging for Review
- Tag two reviewers: either Alice, Nick, or Yuchen Zhang.

### Pull Request (PR) Description
- Write a clear and concise description for each PR.
- Add comments for easier understanding.

### Commenting on Code
- **Requirement**: Comments are required for new code to ensure others can understand it.
- **Future Improvements**: Reference the relevant GitHub issue for any future improvements.

  ![Comment Example](assets/comments.readme.png)

## Run RawPOSIX-Rust

Quick start
Use Develop branch for the most stable behaviour.

```bash
docker build -t --platform <your platform> <image_name> .devcontainer
docker run -it <image_name>

```

This will create a quick container with rustposix build at your local changes.
helpful for exploration and easy testing.

See reference at [Run RustPOSIX Independently](https://github.com/Lind-Project/lind-docs/blob/main/docs/RustPOSIX/Run-Independently.md)

See reference at [Testing and Debugging](https://github.com/Lind-Project/lind-docs/blob/main/docs/RustPOSIX/Testing-and-Debugging.md)

## Development Guideline

* All PRs should be merged to the Develop branch

* Any imports from the standard library or any crates should be done in an interface file