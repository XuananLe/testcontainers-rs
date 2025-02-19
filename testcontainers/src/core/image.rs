use std::{
    collections::BTreeMap,
    env::var,
    fmt::{Debug, Display},
    net::IpAddr,
    time::Duration,
};

use super::ports::Ports;

/// Represents a docker image.
///
/// Implementations are required to implement Default. The default instance of an [`Image`]
/// should have a meaningful configuration! It should be possible to [`run`][docker_run] the default
/// instance of an Image and get back a working container!
///
/// [`Image`]: trait.Image.html
/// [docker_run]: trait.Docker.html#tymethod.run
pub trait Image
where
    Self: Sized + Sync + Send,
    Self::Args: ImageArgs + Clone + Debug + Sync + Send,
{
    /// A type representing the arguments for an Image.
    ///
    /// There are a couple of things regarding the arguments of images:
    ///
    /// 1. Similar to the Default implementation of an Image, the Default instance
    /// of its arguments should be meaningful!
    /// 2. Implementations should be conservative about which arguments they expose. Many times,
    /// users will either go with the default arguments or just override one or two. When defining
    /// the arguments of your image, consider that the whole purpose is to facilitate integration
    /// testing. Only expose those that actually make sense for this case.
    type Args;

    /// The name of the docker image to pull from the Docker Hub registry.
    fn name(&self) -> String;

    /// Implementations are encouraged to include a tag that will not change (i.e. NOT latest)
    /// in order to prevent test code from randomly breaking because the underlying docker
    /// suddenly changed.
    fn tag(&self) -> String;

    /// Returns a list of conditions that need to be met before a started container is considered ready.
    ///
    /// This method is the **🍞 and butter** of the whole testcontainers library. Containers are
    /// rarely instantly available as soon as they are started. Most of them take some time to boot
    /// up.
    ///
    /// The conditions returned from this method are evaluated **in the order** they are returned. Therefore
    /// you most likely want to start with a [`WaitFor::StdOutMessage`] or [`WaitFor::StdErrMessage`] and
    /// potentially follow up with a [`WaitFor::Duration`] in case the container usually needs a little
    /// more time before it is ready.
    fn ready_conditions(&self) -> Vec<WaitFor>;

    /// There are a couple of things regarding the environment variables of images:
    ///
    /// 1. Similar to the Default implementation of an Image, the Default instance
    /// of its environment variables should be meaningful!
    /// 2. Implementations should be conservative about which environment variables they expose. Many times,
    /// users will either go with the default ones or just override one or two. When defining
    /// the environment variables of your image, consider that the whole purpose is to facilitate integration
    /// testing. Only expose those that actually make sense for this case.
    fn env_vars(&self) -> Box<dyn Iterator<Item = (&String, &String)> + '_> {
        Box::new(std::iter::empty())
    }

    /// There are a couple of things regarding the volumes of images:
    ///
    /// 1. Similar to the Default implementation of an Image, the Default instance
    /// of its volumes should be meaningful!
    /// 2. Implementations should be conservative about which volumes they expose. Many times,
    /// users will either go with the default ones or just override one or two. When defining
    /// the volumes of your image, consider that the whole purpose is to facilitate integration
    /// testing. Only expose those that actually make sense for this case.
    fn volumes(&self) -> Box<dyn Iterator<Item = (&String, &String)> + '_> {
        Box::new(std::iter::empty())
    }

    /// Returns the entrypoint this instance was created with.
    fn entrypoint(&self) -> Option<String> {
        None
    }

    /// Returns the ports that needs to be exposed when a container is created.
    ///
    /// This method is useful when there is a need to expose some ports, but there is
    /// no EXPOSE instruction in the Dockerfile of an image.
    fn expose_ports(&self) -> Vec<u16> {
        Default::default()
    }

    /// Returns the commands that needs to be executed after a container is started i.e. commands
    /// to be run in a running container.
    ///
    /// This method is useful when certain re-configuration is required after the start
    /// of container for the container to be considered ready for use in tests.
    #[allow(unused_variables)]
    fn exec_after_start(&self, cs: ContainerState) -> Vec<ExecCommand> {
        Default::default()
    }
}

#[derive(Debug)]
pub struct ExecCommand {
    pub(super) cmd: Vec<String>,
    pub(super) cmd_ready_condition: WaitFor,
    pub(super) container_ready_conditions: Vec<WaitFor>,
}

impl ExecCommand {
    /// Command to be executed
    pub fn new(cmd: Vec<String>) -> Self {
        Self {
            cmd,
            cmd_ready_condition: WaitFor::Nothing,
            container_ready_conditions: vec![],
        }
    }

    /// Conditions to be checked on related container
    pub fn with_container_ready_conditions(mut self, ready_conditions: Vec<WaitFor>) -> Self {
        self.container_ready_conditions = ready_conditions;
        self
    }

    /// Conditions to be checked on executed command output
    pub fn with_cmd_ready_condition(mut self, ready_conditions: WaitFor) -> Self {
        self.cmd_ready_condition = ready_conditions;
        self
    }
}

impl Default for ExecCommand {
    fn default() -> Self {
        Self::new(vec![])
    }
}

#[derive(Debug)]
pub struct ContainerState {
    ports: Ports,
}

impl ContainerState {
    pub fn new(ports: Ports) -> Self {
        Self { ports }
    }

    pub fn host_port_ipv4(&self, internal_port: u16) -> u16 {
        self.ports
            .map_to_host_port_ipv4(internal_port)
            .unwrap_or_else(|| panic!("Container does not have a mapped port for {internal_port}",))
    }

    pub fn host_port_ipv6(&self, internal_port: u16) -> u16 {
        self.ports
            .map_to_host_port_ipv6(internal_port)
            .unwrap_or_else(|| panic!("Container does not have a mapped port for {internal_port}",))
    }
}

pub trait ImageArgs {
    fn into_iterator(self) -> Box<dyn Iterator<Item = String>>;
}

impl ImageArgs for () {
    fn into_iterator(self) -> Box<dyn Iterator<Item = String>> {
        Box::new(vec![].into_iter())
    }
}

#[derive(Debug, Clone)]
pub enum Host {
    Addr(IpAddr),
    HostGateway,
}

impl Display for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Host::Addr(addr) => write!(f, "{addr}"),
            Host::HostGateway => write!(f, "host-gateway"),
        }
    }
}

#[must_use]
#[derive(Debug, Clone)]
pub struct RunnableImage<I: Image> {
    image: I,
    image_args: I::Args,
    image_name: Option<String>,
    image_tag: Option<String>,
    container_name: Option<String>,
    network: Option<String>,
    env_vars: BTreeMap<String, String>,
    hosts: BTreeMap<String, Host>,
    volumes: BTreeMap<String, String>,
    ports: Option<Vec<Port>>,
    privileged: bool,
    shm_size: Option<u64>,
}

impl<I: Image> RunnableImage<I> {
    pub fn image(&self) -> &I {
        &self.image
    }

    pub fn args(&self) -> &I::Args {
        &self.image_args
    }

    pub fn network(&self) -> &Option<String> {
        &self.network
    }

    pub fn container_name(&self) -> &Option<String> {
        &self.container_name
    }

    pub fn env_vars(&self) -> Box<dyn Iterator<Item = (&String, &String)> + '_> {
        Box::new(self.image.env_vars().chain(self.env_vars.iter()))
    }

    pub fn hosts(&self) -> Box<dyn Iterator<Item = (&String, &Host)> + '_> {
        Box::new(self.hosts.iter())
    }

    pub fn volumes(&self) -> Box<dyn Iterator<Item = (&String, &String)> + '_> {
        Box::new(self.image.volumes().chain(self.volumes.iter()))
    }

    pub fn ports(&self) -> &Option<Vec<Port>> {
        &self.ports
    }

    pub fn privileged(&self) -> bool {
        self.privileged
    }

    /// Shared memory size in bytes
    pub fn shm_size(&self) -> Option<u64> {
        self.shm_size
    }

    pub fn entrypoint(&self) -> Option<String> {
        self.image.entrypoint()
    }

    pub fn descriptor(&self) -> String {
        let original_name = self.image.name();
        let original_tag = self.image.tag();

        let name = self.image_name.as_ref().unwrap_or(&original_name);
        let tag = self.image_tag.as_ref().unwrap_or(&original_tag);

        format!("{name}:{tag}")
    }

    pub fn ready_conditions(&self) -> Vec<WaitFor> {
        self.image.ready_conditions()
    }

    pub fn expose_ports(&self) -> Vec<u16> {
        self.image.expose_ports()
    }

    pub fn exec_after_start(&self, cs: ContainerState) -> Vec<ExecCommand> {
        self.image.exec_after_start(cs)
    }
}

impl<I: Image> RunnableImage<I> {
    /// Returns a new RunnableImage with the specified arguments.

    /// # Examples
    /// ```
    /// use testcontainers::{core::RunnableImage, GenericImage};
    ///
    /// let image = GenericImage::default();
    /// let args = vec!["arg1".to_string(), "arg2".to_string()];
    /// let runnable_image = RunnableImage::from(image.clone()).with_args(args.clone());
    ///
    /// assert_eq!(runnable_image.args(), &args);
    ///
    /// let another_runnable_image = RunnableImage::from((image, args));
    ///
    /// assert_eq!(another_runnable_image.args(), runnable_image.args());
    /// ```
    pub fn with_args(self, args: I::Args) -> Self {
        Self {
            image_args: args,
            ..self
        }
    }

    /// Overrides the fully qualified image name (consists of `{domain}/{owner}/{image}`).
    /// Can be used to specify a custom registry or owner.
    pub fn with_name(self, name: impl Into<String>) -> Self {
        Self {
            image_name: Some(name.into()),
            ..self
        }
    }

    /// There is no guarantee that the specified tag for an image would result in a
    /// running container. Users of this API are advised to use this at their own risk.
    pub fn with_tag(self, tag: impl Into<String>) -> Self {
        Self {
            image_tag: Some(tag.into()),
            ..self
        }
    }

    pub fn with_container_name(self, name: impl Into<String>) -> Self {
        Self {
            container_name: Some(name.into()),
            ..self
        }
    }

    pub fn with_network(self, network: impl Into<String>) -> Self {
        Self {
            network: Some(network.into()),
            ..self
        }
    }

    pub fn with_env_var(self, (key, value): (impl Into<String>, impl Into<String>)) -> Self {
        let mut env_vars = self.env_vars;
        env_vars.insert(key.into(), value.into());
        Self { env_vars, ..self }
    }

    pub fn with_host(self, key: impl Into<String>, value: impl Into<Host>) -> Self {
        let mut hosts = self.hosts;
        hosts.insert(key.into(), value.into());
        Self { hosts, ..self }
    }

    pub fn with_volume(self, (orig, dest): (impl Into<String>, impl Into<String>)) -> Self {
        let mut volumes = self.volumes;
        volumes.insert(orig.into(), dest.into());
        Self { volumes, ..self }
    }

    pub fn with_mapped_port<P: Into<Port>>(self, port: P) -> Self {
        let mut ports = self.ports.unwrap_or_default();
        ports.push(port.into());

        Self {
            ports: Some(ports),
            ..self
        }
    }

    pub fn with_privileged(self, privileged: bool) -> Self {
        Self { privileged, ..self }
    }

    pub fn with_shm_size(self, bytes: u64) -> Self {
        Self {
            shm_size: Some(bytes),
            ..self
        }
    }
}

impl<I> From<I> for RunnableImage<I>
where
    I: Image,
    I::Args: Default,
{
    fn from(image: I) -> Self {
        Self::from((image, I::Args::default()))
    }
}

impl<I: Image> From<(I, I::Args)> for RunnableImage<I> {
    fn from((image, image_args): (I, I::Args)) -> Self {
        Self {
            image,
            image_args,
            image_name: None,
            image_tag: None,
            container_name: None,
            network: None,
            env_vars: BTreeMap::default(),
            hosts: BTreeMap::default(),
            volumes: BTreeMap::default(),
            ports: None,
            privileged: false,
            shm_size: None,
        }
    }
}

/// Represents a port mapping between a local port and the internal port of a container.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Port {
    pub local: u16,
    pub internal: u16,
}

/// Represents a condition that needs to be met before a container is considered ready.
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum WaitFor {
    /// An empty condition. Useful for default cases or fallbacks.
    Nothing,
    /// Wait for a message on the stdout stream of the container's logs.
    StdOutMessage { message: String },
    /// Wait for a message on the stderr stream of the container's logs.
    StdErrMessage { message: String },
    /// Wait for a certain amount of time.
    Duration { length: Duration },
    /// Wait for the container's status to become `healthy`.
    Healthcheck,
}

impl WaitFor {
    pub fn message_on_stdout<S: Into<String>>(message: S) -> WaitFor {
        WaitFor::StdOutMessage {
            message: message.into(),
        }
    }

    pub fn message_on_stderr<S: Into<String>>(message: S) -> WaitFor {
        WaitFor::StdErrMessage {
            message: message.into(),
        }
    }

    pub fn seconds(length: u64) -> WaitFor {
        WaitFor::Duration {
            length: Duration::from_secs(length),
        }
    }

    pub fn millis(length: u64) -> WaitFor {
        WaitFor::Duration {
            length: Duration::from_millis(length),
        }
    }

    pub fn millis_in_env_var(name: &'static str) -> WaitFor {
        let additional_sleep_period = var(name).map(|value| value.parse());

        (|| {
            let length = additional_sleep_period.ok()?.ok()?;

            Some(WaitFor::Duration {
                length: Duration::from_millis(length),
            })
        })()
        .unwrap_or(WaitFor::Nothing)
    }
}

impl From<(u16, u16)> for Port {
    fn from((local, internal): (u16, u16)) -> Self {
        Port { local, internal }
    }
}
