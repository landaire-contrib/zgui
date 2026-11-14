//! Choosing an adapter, and refusing to guess when none of them works.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use zgui_render::GpuUnavailable;

use crate::error::Rejection;

/// The environment variable naming the PCI device id to prefer.
///
/// A machine with two adapters renders on whichever one the sort below prefers, and on a hybrid
/// laptop that is not always the one the compositor is on. This is the escape hatch, and it is a
/// preference rather than a filter: an id that matches nothing still selects normally, because a
/// stale value in a shell profile must not turn into "this program has no graphics device".
///
/// The device id this names sorts before each [`AdapterPreference`] of the application.
pub const DEVICE_ID_VARIABLE: &str = "ZGUI_DEVICE_ID";

/// Which adapter an application prefers.
///
/// A preference sorts the adapters of each backend tier. It does not remove an adapter: where the
/// preferred adapter cannot open a device or cannot present to the window, the candidate loop tries
/// the next one. The tiers stay in order, so the GL tier is enumerated only after each adapter of
/// the native tier has failed. [`DEVICE_ID_VARIABLE`] sorts before the preference, and
/// [`BACKENDS_VARIABLE`] selects the backends before the sort.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum AdapterPreference {
    /// The order of this crate: discrete before integrated before virtual before software.
    #[default]
    Automatic,
    /// An adapter whose name contains this text, compared without case, before the other
    /// adapters.
    NameContains(String),
    /// A software adapter, for example lavapipe or llvmpipe, before the other adapters.
    ///
    /// lavapipe is a Vulkan adapter only where
    /// [`prefer_hardware_vulkan`](crate::gpu::loader::prefer_hardware_vulkan) did not exclude it.
    /// llvmpipe is a GL adapter, so it is a candidate only after the native tier has failed, or
    /// where [`BACKENDS_VARIABLE`] selects only GL.
    Software,
    /// An integrated adapter before a discrete one.
    LowPower,
}

impl AdapterPreference {
    /// Returns `true` where this preference names the adapter of `info`.
    ///
    /// [`AdapterPreference::Automatic`] and [`AdapterPreference::LowPower`] rank device types and
    /// name no adapter. The candidate loop tries a software adapter that a preference names in its
    /// tier, and holds back each other software adapter until every tier has been walked.
    pub fn selects(&self, info: &wgpu::AdapterInfo) -> bool {
        match self {
            Self::NameContains(text) => info.name.to_lowercase().contains(&text.to_lowercase()),
            Self::Software => is_software(info),
            Self::Automatic | Self::LowPower => false,
        }
    }
}

/// The environment variable restricting which backends are enumerated at all.
///
/// Accepts `vulkan`, `metal`, `dx12`, `gl`, a comma-separated list of them, or `none`. `none` is
/// what makes the no-device path reachable on a machine that has one: without it, the only way to
/// see what a user with no driver sees is to uninstall the driver.
pub const BACKENDS_VARIABLE: &str = "ZGUI_BACKENDS";

/// The backends enumerated when nothing restricts them.
///
/// Each platform has one primary — Metal on Apple, DX12 on Windows, WebGPU in a browser, Vulkan
/// elsewhere — and GL rides along as the fallback for a machine whose primary has no working
/// driver. In a browser GL is WebGL 2. wgpu reaches
/// GL through EGL, which Apple platforms do not ship, so the Apple set is Metal alone. GL is
/// enumerated only after the primary has failed — see [`tiers`].
pub fn default_backends() -> wgpu::Backends {
    #[cfg(target_vendor = "apple")]
    {
        wgpu::Backends::METAL
    }
    #[cfg(target_os = "windows")]
    {
        wgpu::Backends::DX12 | wgpu::Backends::GL
    }
    #[cfg(target_arch = "wasm32")]
    {
        wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL
    }
    #[cfg(not(any(target_vendor = "apple", target_os = "windows", target_arch = "wasm32")))]
    {
        wgpu::Backends::VULKAN | wgpu::Backends::GL
    }
}

/// How many times a backend set containing GL has been enumerated in this process.
static GL_ENUMERATIONS: AtomicU64 = AtomicU64::new(0);

/// How many times this process has asked a driver for its GL adapters.
///
/// Enumerating GL loads, initialises and interrogates a vendor's EGL and GL cores, which on a
/// discrete card costs more than everything else about choosing an adapter put together. It is a
/// fallback, so on a machine where the primary works this number is zero, and that is a thing a
/// test can assert rather than a thing a profile has to be read for.
pub fn gl_enumerations() -> u64 {
    GL_ENUMERATIONS.load(Ordering::Relaxed)
}

/// The backend sets to enumerate, in the order they are to be tried.
///
/// A fallback constructed before the primary has been tried is not a fallback, it is a tax: the
/// adapters of every requested backend used to be enumerated up front, so a machine that opens a
/// native device every time still paid for its GL cores to be brought up and then dropped unused.
/// So the request is cut into tiers — the native backends, then GL — and a tier is enumerated
/// only once every tier before it has failed to produce a working device.
///
/// A request naming one backend yields one tier, which is what makes [`BACKENDS_VARIABLE`] mean
/// exactly what it says. The empty request yields no tiers at all.
pub fn tiers(backends: wgpu::Backends) -> Vec<wgpu::Backends> {
    let primary = backends - wgpu::Backends::GL;
    let fallback = backends & wgpu::Backends::GL;
    [primary, fallback]
        .into_iter()
        .filter(|tier| !tier.is_empty())
        .collect()
}

/// How many instances containing GL this process has created.
static GL_INSTANCES: AtomicU64 = AtomicU64::new(0);

/// How many times this process has created an instance containing GL.
///
/// Creating one loads and initialises the vendor's EGL, which costs more than 20 MB of resident
/// memory on NVIDIA. On a machine where the primary backend works this number is zero.
pub fn gl_instances() -> u64 {
    GL_INSTANCES.load(Ordering::Relaxed)
}

/// One instance per backend tier, each created when something first asks for it.
///
/// wgpu initialises every backend of an instance when it creates the instance. One instance for
/// all tiers therefore loads the GL driver on every machine, also where the primary tier opens a
/// device. Each tier has its own instance here, so the fallback driver loads only when the
/// candidate loop reaches its tier.
#[derive(Debug)]
pub(crate) struct Instances {
    /// The tiers in the order they are tried, each with its instance once created.
    tiers: Vec<(wgpu::Backends, OnceLock<wgpu::Instance>)>,
}

impl Instances {
    /// The instances of the tiers of `backends`, none of them created yet.
    ///
    /// The empty request keeps one empty tier, so that [`Instances::primary`] has an instance to
    /// give. That instance enumerates no adapters.
    pub(crate) fn new(backends: wgpu::Backends) -> Self {
        let mut tiers = tiers(backends);
        if tiers.is_empty() {
            tiers.push(backends);
        }
        Self {
            tiers: tiers
                .into_iter()
                .map(|tier| (tier, OnceLock::new()))
                .collect(),
        }
    }

    /// One tier of `backends`, served by `instance`, which already exists.
    pub(crate) fn existing(instance: wgpu::Instance, backends: wgpu::Backends) -> Self {
        Self {
            tiers: vec![(backends, OnceLock::from(instance))],
        }
    }

    /// The instance of the first tier, created if necessary.
    pub(crate) fn primary(&self) -> &wgpu::Instance {
        let (tier, instance) = &self.tiers[0];
        instance.get_or_init(|| create(*tier))
    }

    /// Each tier with its instance, in order. The iterator creates an instance when it reaches the
    /// tier, so a loop that stops early creates none of the tiers after it.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (wgpu::Backends, &wgpu::Instance)> {
        self.tiers
            .iter()
            .map(|(tier, instance)| (*tier, instance.get_or_init(|| create(*tier))))
    }
}

/// Creates an instance over `backends`, and counts it if it contains GL.
fn create(backends: wgpu::Backends) -> wgpu::Instance {
    if backends.intersects(wgpu::Backends::GL) {
        GL_INSTANCES.fetch_add(1, Ordering::Relaxed);
    }
    wgpu::Instance::new(instance_descriptor(backends))
}

/// The flags every instance is created with.
///
/// Release builds remove wgpu's indirect-call validation. With it, each new device builds compute
/// pipelines to check indirect arguments, and on NVIDIA that loads a second compiler path: 21 MB of
/// resident memory. The renderer makes no indirect calls. The vector rasteriser makes two indirect
/// dispatches, and its own shaders write their counts: zero after an overflow, at most 8 192
/// otherwise, which is below the dispatch limit. Debug builds keep the check, and
/// `WGPU_VALIDATION_INDIRECT_CALL=1` enables it in a release build.
pub fn instance_flags() -> wgpu::InstanceFlags {
    let mut flags = wgpu::InstanceFlags::from_build_config();
    if !cfg!(debug_assertions) {
        flags.remove(wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL);
    }
    flags.with_env()
}

/// The descriptor of an instance over `backends`.
pub fn instance_descriptor(backends: wgpu::Backends) -> wgpu::InstanceDescriptor {
    wgpu::InstanceDescriptor {
        backends,
        flags: instance_flags(),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    }
}

/// The backends to enumerate, after the environment has had its say.
pub fn requested_backends() -> wgpu::Backends {
    match std::env::var(BACKENDS_VARIABLE) {
        Err(_) => default_backends(),
        Ok(value) => parse_backends(&value),
    }
}

/// Parses the backend list an environment variable carries.
///
/// An unrecognised word contributes nothing, so `ZGUI_BACKENDS=nonsense` is the same request as
/// `none`: a typo that quietly fell back to every backend would make this variable useless for
/// reproducing a machine with no usable device.
pub fn parse_backends(value: &str) -> wgpu::Backends {
    let mut backends = wgpu::Backends::empty();
    for word in value.split(',') {
        match word.trim().to_ascii_lowercase().as_str() {
            "vulkan" => backends |= wgpu::Backends::VULKAN,
            "metal" => backends |= wgpu::Backends::METAL,
            "dx12" | "d3d12" => backends |= wgpu::Backends::DX12,
            "gl" | "gles" | "opengl" | "webgl" | "webgl2" => backends |= wgpu::Backends::GL,
            "webgpu" => backends |= wgpu::Backends::BROWSER_WEBGPU,
            "all" => backends |= default_backends(),
            _ => {}
        }
    }
    backends
}

/// The preferred device id, if one was named.
pub fn preferred_device_id() -> Option<u32> {
    let value = std::env::var(DEVICE_ID_VARIABLE).ok()?;
    let value = value.trim();
    let parsed = match value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => value.parse(),
    };
    parsed.ok()
}

/// How good a candidate an adapter looks, before anything has been created from it.
///
/// A lower key sorts first. The fields compare in their order of declaration, and the variants of
/// each field compare in their order of declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SortKey {
    /// Whether the adapter has the device id that [`DEVICE_ID_VARIABLE`] names.
    device_id: DeviceIdMatch,
    /// Whether the adapter matches the [`AdapterPreference`].
    preference: PreferenceMatch,
    /// The rank of the device type.
    device_type: DeviceTypeRank,
    /// The rank of the backend.
    backend: BackendRank,
}

/// Whether an adapter has the device id that [`DEVICE_ID_VARIABLE`] names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum DeviceIdMatch {
    /// The adapter has the named device id.
    Named,
    /// No device id is named, or the adapter has a different one.
    Other,
}

/// Whether an adapter matches the [`AdapterPreference`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum PreferenceMatch {
    /// The adapter matches the preference.
    Matched,
    /// The adapter does not match the preference.
    Unmatched,
}

/// The rank of an adapter's device type under an [`AdapterPreference`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum DeviceTypeRank {
    /// A discrete GPU, or an integrated GPU for [`AdapterPreference::LowPower`].
    PreferredGpu,
    /// An integrated GPU, or a discrete GPU for [`AdapterPreference::LowPower`].
    OtherGpu,
    /// A virtual GPU.
    Virtual,
    /// A software adapter.
    Cpu,
    /// A device type that wgpu does not know.
    Other,
}

/// The rank of an adapter's backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum BackendRank {
    /// Vulkan, Metal or DX12.
    Native,
    /// GL.
    Gl,
    /// A backend that is neither native nor GL.
    Other,
}

/// The sort key of an adapter.
///
/// The order is: the device id the environment asked for, then the adapter that `preference`
/// names, then discrete before integrated before virtual before software (integrated first for
/// [`AdapterPreference::LowPower`]), then a native backend before GL. It is only a *sort*: every
/// candidate is still tried in turn, because the capabilities an adapter reports are not always
/// the capabilities a device created from it has.
pub fn sort_key(
    info: &wgpu::AdapterInfo,
    preferred: Option<u32>,
    preference: &AdapterPreference,
) -> SortKey {
    let device_id = if Some(info.device) == preferred {
        DeviceIdMatch::Named
    } else {
        DeviceIdMatch::Other
    };
    let matched = match preference {
        AdapterPreference::Automatic | AdapterPreference::LowPower => true,
        AdapterPreference::NameContains(_) | AdapterPreference::Software => {
            preference.selects(info)
        }
    };
    let preference_match = if matched {
        PreferenceMatch::Matched
    } else {
        PreferenceMatch::Unmatched
    };
    let device_type = match (info.device_type, preference) {
        (wgpu::DeviceType::IntegratedGpu, AdapterPreference::LowPower) => {
            DeviceTypeRank::PreferredGpu
        }
        (wgpu::DeviceType::DiscreteGpu, AdapterPreference::LowPower) => DeviceTypeRank::OtherGpu,
        (wgpu::DeviceType::DiscreteGpu, _) => DeviceTypeRank::PreferredGpu,
        (wgpu::DeviceType::IntegratedGpu, _) => DeviceTypeRank::OtherGpu,
        (wgpu::DeviceType::VirtualGpu, _) => DeviceTypeRank::Virtual,
        (wgpu::DeviceType::Cpu, _) => DeviceTypeRank::Cpu,
        (wgpu::DeviceType::Other, _) => DeviceTypeRank::Other,
    };
    let backend = match info.backend {
        wgpu::Backend::Vulkan | wgpu::Backend::Metal | wgpu::Backend::Dx12 => BackendRank::Native,
        wgpu::Backend::Gl => BackendRank::Gl,
        _ => BackendRank::Other,
    };

    SortKey {
        device_id,
        preference: preference_match,
        device_type,
        backend,
    }
}

/// Returns `true` if this adapter rasterises on the processor.
///
/// Such an adapter is a last resort rather than a tier: it draws every pixel on the cores the
/// application is also running on, and it is offered by a driver that is installed on a great many
/// machines that also have a real graphics card. `llvmpipe` and `lavapipe` are the two this meets
/// — Mesa's software renderers for GL and for Vulkan.
///
/// A virtualised device answers `false`. It is a real card behind a hypervisor, and it draws on
/// that card.
pub fn is_software(info: &wgpu::AdapterInfo) -> bool {
    matches!(info.device_type, wgpu::DeviceType::Cpu)
}

/// A one-line description of an adapter, for a rejection list or a startup line.
pub fn describe(info: &wgpu::AdapterInfo) -> String {
    format!(
        "{} ({:?}, {:?}, device 0x{:04x}, driver {} {})",
        info.name, info.device_type, info.backend, info.device, info.driver, info.driver_info
    )
}

/// Every adapter of `backends`, best-looking first under `preference`.
///
/// This is one tier's worth of enumeration, not the whole request: see [`tiers`].
pub fn candidates(
    instance: &wgpu::Instance,
    backends: wgpu::Backends,
    preference: &AdapterPreference,
) -> Vec<wgpu::Adapter> {
    if backends.intersects(wgpu::Backends::GL) {
        GL_ENUMERATIONS.fetch_add(1, Ordering::Relaxed);
    }
    let preferred = preferred_device_id();
    let mut adapters = futures::executor::block_on(instance.enumerate_adapters(backends));
    // Cached, because a name preference lower-cases two strings for each key.
    adapters.sort_by_cached_key(|adapter| sort_key(&adapter.get_info(), preferred, preference));
    adapters
}

/// Every adapter of `backends`, in the order that the candidate loop tries them under
/// `preference`.
///
/// Use this for a diagnostic list of the graphics adapters of a machine. It enumerates each tier,
/// also the GL tier, which the candidate loop enumerates only after the native tier has failed.
/// [`requested_backends`] gives the backends that the environment allows.
///
/// ```no_run
/// use zgui_render_wgpu::gpu::adapter;
///
/// for info in adapter::enumerate(adapter::requested_backends(), &Default::default()) {
///     println!("{}", adapter::describe(&info));
/// }
/// ```
pub fn enumerate(
    backends: wgpu::Backends,
    preference: &AdapterPreference,
) -> Vec<wgpu::AdapterInfo> {
    let mut infos = Vec::new();
    for tier in tiers(backends) {
        let instance = create(tier);
        infos.extend(
            candidates(&instance, tier, preference)
                .iter()
                .map(wgpu::Adapter::get_info),
        );
    }

    infos
}

/// The failure to report when the candidate loop rejected everything.
///
/// It carries every adapter that was considered and why, because "no usable graphics device" with
/// no list is a bug report nobody can act on — and because the alternative, quietly rendering
/// offscreen, produces a window that appears and never paints.
pub fn unavailable(rejections: Vec<Rejection>) -> GpuUnavailable {
    rejections
        .into_iter()
        .fold(GpuUnavailable::new(), |failure, rejection| {
            failure.rejected(describe(&rejection.adapter), rejection)
        })
}

#[cfg(test)]
mod tests {
    use super::{
        AdapterPreference, describe, instance_flags, is_software, parse_backends, sort_key, tiers,
        unavailable,
    };
    use crate::error::{PresentError, Rejection, RejectionReason};

    #[test]
    fn only_debug_instances_validate_indirect_calls() {
        // The environment overrides the build's choice.
        if std::env::var_os("WGPU_VALIDATION_INDIRECT_CALL").is_some() {
            return;
        }
        assert_eq!(
            instance_flags().contains(wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL),
            cfg!(debug_assertions)
        );
    }

    #[test]
    fn gl_is_a_tier_of_its_own_behind_the_native_backends() {
        assert_eq!(
            tiers(wgpu::Backends::VULKAN | wgpu::Backends::GL),
            vec![wgpu::Backends::VULKAN, wgpu::Backends::GL],
            "the fallback is a second tier, not part of the first"
        );
        assert_eq!(
            tiers(wgpu::Backends::METAL | wgpu::Backends::GL),
            vec![wgpu::Backends::METAL, wgpu::Backends::GL],
            "every native backend outranks the fallback, whatever the platform"
        );
    }

    #[test]
    fn a_request_for_one_backend_is_one_tier() {
        assert_eq!(tiers(wgpu::Backends::GL), vec![wgpu::Backends::GL]);
        assert_eq!(tiers(wgpu::Backends::VULKAN), vec![wgpu::Backends::VULKAN]);
        assert!(tiers(wgpu::Backends::empty()).is_empty());
    }

    /// An adapter description with the fields the sort reads.
    fn info(
        device: u32,
        device_type: wgpu::DeviceType,
        backend: wgpu::Backend,
    ) -> wgpu::AdapterInfo {
        named("test", device, device_type, backend)
    }

    /// An adapter description named `name`.
    fn named(
        name: &str,
        device: u32,
        device_type: wgpu::DeviceType,
        backend: wgpu::Backend,
    ) -> wgpu::AdapterInfo {
        wgpu::AdapterInfo {
            name: name.to_owned(),
            vendor: 0,
            device,
            device_type,
            driver: String::new(),
            driver_info: String::new(),
            backend,
            device_pci_bus_id: String::new(),
            subgroup_min_size: 32,
            subgroup_max_size: 32,
            transient_saves_memory: Some(false),
            limit_bucket: None,
        }
    }

    /// A machine with a discrete, an integrated and a software adapter, in an order the sort has
    /// to change.
    fn machine() -> Vec<wgpu::AdapterInfo> {
        vec![
            named(
                "llvmpipe (LLVM 19.1.0, 256 bits)",
                3,
                wgpu::DeviceType::Cpu,
                wgpu::Backend::Vulkan,
            ),
            named(
                "Intel(R) Graphics (RPL-S)",
                2,
                wgpu::DeviceType::IntegratedGpu,
                wgpu::Backend::Vulkan,
            ),
            named(
                "NVIDIA GeForce RTX 4090",
                1,
                wgpu::DeviceType::DiscreteGpu,
                wgpu::Backend::Vulkan,
            ),
        ]
    }

    /// The names of `infos` in the order the candidate loop tries them.
    fn order(
        mut infos: Vec<wgpu::AdapterInfo>,
        preferred: Option<u32>,
        preference: &AdapterPreference,
    ) -> Vec<String> {
        infos.sort_by_key(|info| sort_key(info, preferred, preference));
        infos.into_iter().map(|info| info.name).collect()
    }

    #[test]
    fn a_named_device_outranks_a_better_one() {
        let automatic = AdapterPreference::Automatic;
        let discrete = info(1, wgpu::DeviceType::DiscreteGpu, wgpu::Backend::Vulkan);
        let integrated = info(2, wgpu::DeviceType::IntegratedGpu, wgpu::Backend::Vulkan);
        assert!(sort_key(&discrete, None, &automatic) < sort_key(&integrated, None, &automatic));
        assert!(
            sort_key(&integrated, Some(2), &automatic) < sort_key(&discrete, Some(2), &automatic)
        );
    }

    #[test]
    fn vulkan_outranks_gl_at_the_same_device_type() {
        let automatic = AdapterPreference::Automatic;
        let vulkan = info(1, wgpu::DeviceType::DiscreteGpu, wgpu::Backend::Vulkan);
        let gl = info(1, wgpu::DeviceType::DiscreteGpu, wgpu::Backend::Gl);
        assert!(sort_key(&vulkan, None, &automatic) < sort_key(&gl, None, &automatic));
    }

    #[test]
    fn software_sorts_last_but_is_still_a_candidate() {
        let automatic = AdapterPreference::Automatic;
        let cpu = info(1, wgpu::DeviceType::Cpu, wgpu::Backend::Vulkan);
        let virtualised = info(1, wgpu::DeviceType::VirtualGpu, wgpu::Backend::Vulkan);
        assert!(sort_key(&virtualised, None, &automatic) < sort_key(&cpu, None, &automatic));
    }

    #[test]
    fn the_automatic_order_puts_the_discrete_adapter_first() {
        assert_eq!(
            order(machine(), None, &AdapterPreference::Automatic),
            [
                "NVIDIA GeForce RTX 4090",
                "Intel(R) Graphics (RPL-S)",
                "llvmpipe (LLVM 19.1.0, 256 bits)",
            ]
        );
    }

    #[test]
    fn a_name_preference_puts_the_matching_adapter_first() {
        let preference = AdapterPreference::NameContains("intel".to_owned());
        assert_eq!(
            order(machine(), None, &preference),
            [
                "Intel(R) Graphics (RPL-S)",
                "NVIDIA GeForce RTX 4090",
                "llvmpipe (LLVM 19.1.0, 256 bits)",
            ]
        );
    }

    #[test]
    fn a_name_that_matches_nothing_keeps_the_automatic_order() {
        let preference = AdapterPreference::NameContains("radeon".to_owned());
        assert_eq!(
            order(machine(), None, &preference),
            order(machine(), None, &AdapterPreference::Automatic)
        );
    }

    #[test]
    fn a_software_preference_puts_the_cpu_adapter_first() {
        assert_eq!(
            order(machine(), None, &AdapterPreference::Software)[0],
            "llvmpipe (LLVM 19.1.0, 256 bits)"
        );
    }

    #[test]
    fn a_software_preference_without_a_cpu_adapter_keeps_the_automatic_order() {
        let hardware: Vec<wgpu::AdapterInfo> = machine()
            .into_iter()
            .filter(|info| info.device_type != wgpu::DeviceType::Cpu)
            .collect();
        assert_eq!(
            order(hardware.clone(), None, &AdapterPreference::Software),
            order(hardware, None, &AdapterPreference::Automatic)
        );
    }

    #[test]
    fn a_low_power_preference_puts_the_integrated_adapter_first() {
        assert_eq!(
            order(machine(), None, &AdapterPreference::LowPower),
            [
                "Intel(R) Graphics (RPL-S)",
                "NVIDIA GeForce RTX 4090",
                "llvmpipe (LLVM 19.1.0, 256 bits)",
            ]
        );
    }

    #[test]
    fn the_environment_device_id_outranks_the_preference() {
        let preference = AdapterPreference::NameContains("intel".to_owned());
        assert_eq!(
            order(machine(), Some(1), &preference)[0],
            "NVIDIA GeForce RTX 4090"
        );
    }

    /// The sort settles one tier, and this is what holds an adapter back past all of them.
    ///
    /// The two answers differ where it matters: a machine can carry a software Vulkan driver
    /// beside a card whose only driver is OpenGL, and a tier order alone puts such a machine on
    /// its processor.
    #[test]
    fn only_a_processor_adapter_is_software() {
        assert!(is_software(&info(
            1,
            wgpu::DeviceType::Cpu,
            wgpu::Backend::Vulkan
        )));
        for real in [
            wgpu::DeviceType::DiscreteGpu,
            wgpu::DeviceType::IntegratedGpu,
            // A real card behind a hypervisor, which draws on that card.
            wgpu::DeviceType::VirtualGpu,
        ] {
            assert!(
                !is_software(&info(1, real, wgpu::Backend::Gl)),
                "{real:?} draws on a card"
            );
        }
    }

    #[test]
    fn an_unrecognised_backend_list_asks_for_nothing() {
        assert_eq!(parse_backends("vulkan"), wgpu::Backends::VULKAN);
        assert_eq!(parse_backends("metal"), wgpu::Backends::METAL);
        assert_eq!(parse_backends("dx12"), wgpu::Backends::DX12);
        assert_eq!(
            parse_backends("gl, vulkan"),
            wgpu::Backends::VULKAN | wgpu::Backends::GL
        );
        assert!(parse_backends("none").is_empty());
        assert!(parse_backends("nonsense").is_empty());
    }

    #[test]
    fn the_failure_lists_every_rejection_in_order() {
        let first = info(1, wgpu::DeviceType::DiscreteGpu, wgpu::Backend::Vulkan);
        let second = info(2, wgpu::DeviceType::IntegratedGpu, wgpu::Backend::Vulkan);
        let failure = unavailable(vec![
            Rejection {
                adapter: first.clone(),
                reason: RejectionReason::ExtensionsRefused,
            },
            Rejection {
                adapter: second,
                reason: PresentError::Incompatible.into(),
            },
        ]);
        assert_eq!(failure.candidates.len(), 2);
        assert_eq!(failure.candidates[0].name, describe(&first));
        let second = failure.candidates[1].reason.downcast_ref::<Rejection>();
        assert!(
            matches!(
                second.map(|rejection| &rejection.reason),
                Some(RejectionReason::Present(PresentError::Incompatible))
            ),
            "{:?}",
            failure.candidates[1]
        );
    }

    #[test]
    fn a_description_names_the_backend_and_the_device() {
        let text = describe(&info(
            0x2216,
            wgpu::DeviceType::DiscreteGpu,
            wgpu::Backend::Gl,
        ));
        assert!(text.contains("Gl"), "{text}");
        assert!(text.contains("0x2216"), "{text}");
    }
}
