use std::collections::BTreeMap;
use std::fs::{create_dir_all, File};
use std::io::Write;
use std::path::PathBuf;

struct DMIHelper<'a> {
    path_root: &'a PathBuf,
}

impl<'a> DMIHelper<'_> {
    fn new(path_root: &'a PathBuf) -> DMIHelper<'a> {
        DMIHelper { path_root }
    }

    fn get_dmi_field(&self, field_name: &str) -> Option<String> {
        let mut path = PathBuf::from(self.path_root.clone());
        path.push("sys/class/dmi/id");
        path.push(field_name);

        std::fs::read_to_string(&path)
            .map(|s| s.trim().to_string())
            .ok()
    }

    fn dmi_product_name(&self) -> Option<String> {
        // TODO: calculate once and store
        // TODO: container check
        self.get_dmi_field("product_name")
    }

    fn dmi_product_serial(&self) -> Option<String> {
        self.get_dmi_field("product_serial")
    }
}

struct RsIdentify {
    // Paths
    path_root: PathBuf,
    cfg_out: PathBuf,

    // DMI values
    dmi_product_name: Option<String>,
    dmi_product_serial: Option<String>,
}

impl RsIdentify {
    // Setup
    fn new(path_root: PathBuf) -> RsIdentify {
        let mut cfg_out = PathBuf::from(path_root.clone());
        cfg_out.push("run/cloud-init/cloud.cfg");

        let dmi_helper = DMIHelper::new(&path_root);
        let dmi_product_name = dmi_helper.dmi_product_name();
        let dmi_product_serial = dmi_helper.dmi_product_serial();

        // Emit our paths/settings
        println!("PATH_ROOT: {}", path_root.display());
        println!("CFG_OUT: {}", cfg_out.display());

        RsIdentify {
            path_root,
            cfg_out,
            dmi_product_name,
            dmi_product_serial,
        }
    }

    fn from_env() -> RsIdentify {
        let path_root = match std::env::var("PATH_ROOT") {
            Ok(val) => PathBuf::from(&val),
            Err(_) => std::process::exit(1),
        };
        RsIdentify::new(path_root)
    }

    // Datasource checks
    fn dscheck_AliYun(&self) -> bool {
        // TODO: seed directory checks
        self.dmi_product_name == Some("Alibaba Cloud ECS".to_string())
    }

    fn dscheck_ConfigDrive(&self) -> bool {
        let mut meta_data_path = PathBuf::from(self.path_root.clone());

        meta_data_path.push("var/lib/cloud/seed/config_drive/openstack/latest/meta_data.json");

        meta_data_path.exists()
    }

    fn dscheck_Exoscale(&self) -> bool {
        // TEST GAP: I didn't need to implement Exoscale support
        self.dmi_product_name == Some("Exoscale".to_string())
    }

    fn dscheck_GCE(&self) -> bool {
        self.dmi_product_name == Some("Google Compute Engine".to_string())
            || self
                .dmi_product_serial
                .as_ref()
                .map(|serial| serial.starts_with("GoogleCloud"))
                .unwrap_or(false)
    }

    fn dscheck_NoCloud(&self) -> bool {
        let mut user_data_path = PathBuf::from(self.path_root.clone());
        let mut meta_data_path = PathBuf::from(self.path_root.clone());

        user_data_path.push("var/lib/cloud/seed/nocloud/user-data");
        meta_data_path.push("var/lib/cloud/seed/nocloud/meta-data");

        if user_data_path.exists() && meta_data_path.exists() {
            return true;
        }

        // TEST GAP: nocloud and nocloud-net are not tested for both seed types
        let mut writable_user_data_path = PathBuf::from(self.path_root.clone());
        let mut writable_meta_data_path = PathBuf::from(self.path_root.clone());

        writable_user_data_path
            .push("writable/system-data/var/lib/cloud/seed/nocloud-net/user-data");
        writable_meta_data_path
            .push("writable/system-data/var/lib/cloud/seed/nocloud-net/meta-data");

        writable_user_data_path.exists() && writable_meta_data_path.exists()
    }

    // Output
    fn write_cfg_out(self, datasource_list: Vec<String>) {
        create_dir_all(self.cfg_out.parent().unwrap()).unwrap();
        let mut file = match File::create(&self.cfg_out) {
            Err(why) => panic!(
                "couldn't create {}: {}",
                self.cfg_out.display(),
                why.to_string()
            ),
            Ok(file) => file,
        };
        let mut map = BTreeMap::new();
        map.insert("datasource_list".to_string(), datasource_list);
        if file
            .write_all(serde_yaml::to_string(&map).unwrap().as_bytes())
            .is_err()
        {
            std::process::exit(1);
        };
    }

    fn get_datasource_list_from_path(&self, path: &PathBuf) -> Option<Vec<String>> {
        let file = match File::open(&path) {
            Err(_) => return None,
            Ok(file) => file,
        };
        let config: serde_yaml::Mapping = serde_yaml::from_reader(file).unwrap();
        config
            .get(&serde_yaml::Value::from("datasource_list"))
            .map(|datasource_list| {
                datasource_list
                    .as_sequence()
                    .unwrap()
                    .iter()
                    .filter_map(|value| value.as_str().map(|s| s.to_string()))
                    .collect()
            })
    }

    fn get_datasource_list(&self) -> Vec<String> {
        // Set up all our paths first
        let mut etc_cloud_path = PathBuf::from(self.path_root.clone());
        etc_cloud_path.push("etc/cloud/cloud.cfg");
        let mut etc_cloud_d_path = PathBuf::from(self.path_root.clone());
        etc_cloud_d_path.push("etc/cloud/cloud.cfg.d");
        let mut cloud_d_paths: Vec<PathBuf> = match std::fs::read_dir(etc_cloud_d_path) {
            Err(_) => vec![],
            Ok(read_dir) => read_dir
                .filter_map(|dir_entry| dir_entry.ok().map(|dir_entry| dir_entry.path()))
                .collect(),
        };
        cloud_d_paths.sort();

        // Find the latest definition of datasource_list and use that
        // TEST GAP: the tests don't exercise checking cloud.cfg itself
        let mut list = self.get_datasource_list_from_path(&etc_cloud_path);
        for cloud_d_path in cloud_d_paths {
            list = self.get_datasource_list_from_path(&cloud_d_path).or(list);
        }
        list.unwrap_or(vec![
            "AliYun".to_string(),
            "ConfigDrive".to_string(),
            "Exoscale".to_string(),
            "GCE".to_string(),
            "NoCloud".to_string(),
        ])
    }

    // Identify
    fn identify(self) {
        // Identify!
        let input_datasource_list = self.get_datasource_list();

        let mut output_datasource_list = vec![];
        for candidate_datasource in input_datasource_list {
            let ds_applies = match candidate_datasource.as_str() {
                // TEST GAP: These DSes have no tests: CloudStack, CloudSigma, Exoscale, MAAS
                "AliYun" => self.dscheck_AliYun(),
                "ConfigDrive" => self.dscheck_ConfigDrive(),
                "Exoscale" => self.dscheck_Exoscale(),
                "GCE" => self.dscheck_GCE(),
                "NoCloud" => self.dscheck_NoCloud(),
                _ => false,
            };
            println!("{}", candidate_datasource);
            if ds_applies {
                output_datasource_list.push(candidate_datasource);
            }
        }

        if !output_datasource_list.contains(&"None".to_string()) {
            output_datasource_list.push("None".to_string());
        };

        // Persist
        self.write_cfg_out(output_datasource_list);
    }
}

// Datasource checks

fn main() {
    // Determine our paths/settings
    let rs_identify = RsIdentify::from_env();
    rs_identify.identify()
}
