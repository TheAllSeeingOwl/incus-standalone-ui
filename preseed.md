# Incus Preseed

Preseed is a way to provide initial configuration to Incus in one shot via a YAML blob, rather than running many separate commands. It mirrors the REST API structure.

```bash
incus admin init --preseed < config.yaml
```

---

## What Preseed Can Configure

- **Storage pools** — driver, source path, size, and driver-specific config
- **Networks** — bridges, physical, macvlan, OVN, with full config options
- **Profiles** — default devices and config applied to instances
- **Projects** — resource limits and feature flags per project
- **Cluster** — cluster name, member join tokens, member roles

## What Preseed Cannot Do

- **Create or configure instances** — preseed is infrastructure-level only; it does not create containers or VMs
- **Assign profiles to instances** — profiles exist after preseed, but attaching them to instances must be done separately
- **Host-side hardware setup** — IOMMU, VFIO binding, kernel parameters, and kernel modules are outside Incus entirely and must be configured via the OS (e.g. NixOS `boot.kernelParams`)
- **Idempotent updates** — preseed is designed for first-run initialization; re-running it against existing state will error on conflicts or silently skip resources depending on the Incus version
- **Delete resources** — preseed only creates; it cannot remove existing pools, networks, or profiles
- **Secret management** — certificates and passwords appear in plain text; integrate with a secrets manager separately

---

## Examples

### Minimal single-node setup

```yaml
networks:
  - name: incusbr0
    type: bridge
    config:
      ipv4.address: 10.0.0.1/24
      ipv4.nat: "true"
      ipv6.address: none

storage_pools:
  - name: default
    driver: dir

profiles:
  - name: default
    devices:
      eth0:
        name: eth0
        network: incusbr0
        type: nic
      root:
        path: /
        pool: default
        type: disk
```

### ZFS storage pool

```yaml
storage_pools:
  - name: zfs-pool
    driver: zfs
    config:
      zfs.pool_name: tank/incus
      size: 100GiB
```

### Isolated project with resource limits

```yaml
projects:
  - name: staging
    config:
      features.images: "true"
      features.profiles: "true"
      limits.cpu: "8"
      limits.memory: 16GiB
      limits.instances: "10"
```

### GPU passthrough profile

Host-side kernel config must be set outside Incus (e.g. in NixOS):

```nix
boot.kernelParams = [ "intel_iommu=on" "iommu=pt" ];
boot.kernelModules = [ "vfio" "vfio_pci" "vfio_iommu_type1" ];
boot.extraModprobeConfig = "options vfio-pci ids=10de:1234,10de:5678";
```

Then the profile referencing the PCI address:

```yaml
profiles:
  - name: gpu-passthrough
    config:
      security.privileged: "false"
    devices:
      gpu0:
        type: gpu
        pci: "0000:03:00.0"
```

For a raw PCI device (non-GPU):

```yaml
profiles:
  - name: pci-passthrough
    devices:
      mydevice:
        type: pci
        address: "0000:03:00.0"
```

### OVN network with uplink

```yaml
networks:
  - name: UPLINK
    type: physical
    config:
      parent: eth0
      ipv4.gateway: 192.168.1.1/24
      ipv6.gateway: none
  - name: ovn-net
    type: ovn
    config:
      network: UPLINK
      ipv4.address: 10.10.0.1/24
      ipv4.nat: "true"
```

### Clustering (first member)

```yaml
cluster:
  server_name: node1
  enabled: true
```

Joining an existing cluster:

```yaml
cluster:
  server_name: node2
  server_address: 10.0.0.2:8443
  cluster_address: 10.0.0.1:8443
  cluster_certificate: |
    -----BEGIN CERTIFICATE-----
    ...
    -----END CERTIFICATE-----
  cluster_token: <join-token>
```

---

## NixOS Integration

NixOS exposes preseed via `services.incus.preseed`. It runs `incus admin init --preseed` on first initialization only.

```nix
services.incus = {
  enable = true;
  preseed = {
    networks = [{
      name = "incusbr0";
      type = "bridge";
      config = {
        "ipv4.address" = "10.0.0.1/24";
        "ipv4.nat" = "true";
        "ipv6.address" = "none";
      };
    }];
    storage_pools = [{
      name = "default";
      driver = "zfs";
      config."zfs.pool_name" = "tank/incus";
    }];
    profiles = [{
      name = "default";
      devices = {
        eth0 = { name = "eth0"; network = "incusbr0"; type = "nic"; };
        root = { path = "/"; pool = "default"; type = "disk"; };
      };
    }];
  };
};
```

For day-2 configuration (adding resources after initial setup), use a oneshot systemd service that calls the `incus` CLI idempotently instead of relying on preseed.
