// Copyright 2020 Daniel Watkins
//
// Use of this source code is governed by the CNPLv4 license that can be found in LICENSE.txt

use std::collections::BTreeMap;
use std::fs::{create_dir_all, File};
use std::io::Write;
use std::path::PathBuf;

struct RsIdentify {
    // Paths
    path_root: PathBuf,
    cfg_out: PathBuf,

    dmi_values: BTreeMap<String, Option<String>>,
}

impl RsIdentify {
    // Setup
    fn new(path_root: PathBuf) -> RsIdentify {
        let mut cfg_out = PathBuf::from(path_root.clone());
        cfg_out.push("run/cloud-init/cloud.cfg");

        // Emit our paths/settings
        println!("PATH_ROOT: {}", path_root.display());
        println!("CFG_OUT: {}", cfg_out.display());

        RsIdentify {
            path_root,
            cfg_out,
            dmi_values: BTreeMap::new(),
        }
    }

    fn from_env() -> RsIdentify {
        let path_root = match std::env::var("PATH_ROOT") {
            Ok(val) => PathBuf::from(&val),
            Err(_) => PathBuf::from("/"),
        };
        RsIdentify::new(path_root)
    }

    // DMI caching
    fn get_dmi_field(&mut self, field_name: &str) -> &Option<String> {
        if !self.dmi_values.contains_key(field_name) {
            let mut path = PathBuf::from(self.path_root.clone());
            path.push("sys/class/dmi/id");
            path.push(field_name);

            let value = std::fs::read_to_string(&path)
                .map(|s| s.trim().to_string())
                .ok();
            self.dmi_values.insert(field_name.to_string(), value);
        }
        self.dmi_values.get(field_name).unwrap()
    }

    fn dmi_chassis_asset_tag(&mut self) -> &Option<String> {
        self.get_dmi_field("chassis_asset_tag")
    }

    fn dmi_product_name(&mut self) -> &Option<String> {
        // TODO: container check
        self.get_dmi_field("product_name")
    }

    fn dmi_product_serial(&mut self) -> &Option<String> {
        self.get_dmi_field("product_serial")
    }

    fn dmi_product_uuid(&mut self) -> &Option<String> {
        self.get_dmi_field("product_uuid")
    }

    // Helpers
    fn seed_path_exists(&self, prefix: Option<&str>, seed_type: &str, filename: &str) -> bool {
        let mut seed_path = PathBuf::from(self.path_root.clone());
        if let Some(prefix) = prefix {
            seed_path.push(prefix);
        }
        seed_path.push("var/lib/cloud/seed");
        seed_path.push(seed_type);
        seed_path.push(filename);
        seed_path.exists()
    }

    // Datasource checks
    #[allow(non_snake_case)]
    fn dscheck_AliYun(&mut self) -> bool {
        // TEST GAP: seed directory checks
        self.dmi_product_name() == &Some("Alibaba Cloud ECS".to_string())
    }

    #[allow(non_snake_case)]
    fn dscheck_Azure(&mut self) -> bool {
        if self.seed_path_exists(None, "azure", "ovf-env.xml") {
            return true;
        }
        self.dmi_chassis_asset_tag() == &Some("7783-7084-3265-9085-8269-3286-77".to_string())
    }

    #[allow(non_snake_case)]
    fn dscheck_ConfigDrive(&self) -> bool {
        self.seed_path_exists(None, "config_drive", "openstack/latest/meta_data.json")
    }

    #[allow(non_snake_case)]
    fn dscheck_Ec2(&mut self) -> bool {
        // TEST_GAP: One of serial or UUID can be missing
        // TEST GAP: Serial and UUID equality is not exercised
        let serial = self
            .dmi_product_serial()
            .as_ref()
            .map(|s| s.to_ascii_lowercase());
        let uuid = self
            .dmi_product_uuid()
            .as_ref()
            .map(|s| s.to_ascii_lowercase());
        serial
            .as_ref()
            .map(|s| s.starts_with("ec2"))
            .unwrap_or(false)
            && uuid.as_ref().map(|s| s.starts_with("ec2")).unwrap_or(false)
            && serial == uuid
    }

    #[allow(non_snake_case)]
    fn dscheck_Exoscale(&mut self) -> bool {
        // TEST GAP: I didn't need to implement Exoscale support
        self.dmi_product_name() == &Some("Exoscale".to_string())
    }

    #[allow(non_snake_case)]
    fn dscheck_GCE(&mut self) -> bool {
        self.dmi_product_name() == &Some("Google Compute Engine".to_string())
            || self
                .dmi_product_serial()
                .as_ref()
                .map(|serial| serial.starts_with("GoogleCloud"))
                .unwrap_or(false)
    }

    #[allow(non_snake_case)]
    fn dscheck_NoCloud(&self) -> bool {
        // TEST GAP: nocloud and nocloud-net are not tested for both writable and regular paths
        for seed_type in &["nocloud", "nocloud-net"] {
            if self.seed_path_exists(None, seed_type, "user-data")
                && self.seed_path_exists(None, seed_type, "meta-data")
            {
                return true;
            }

            if self.seed_path_exists(Some("writable/system-data"), seed_type, "user-data")
                && self.seed_path_exists(Some("writable/system-data"), seed_type, "meta-data")
            {
                return true;
            }
        }
        false
    }

    #[allow(non_snake_case)]
    fn dscheck_Oracle(&mut self) -> bool {
        self.dmi_chassis_asset_tag() == &Some("OracleCloud.com".to_string())
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
        let config: serde_yaml::Mapping = match serde_yaml::from_reader(file) {
            Err(_) => return None,
            Ok(result) => result,
        };
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
            "Azure".to_string(),
            "ConfigDrive".to_string(),
            "Ec2".to_string(),
            "Exoscale".to_string(),
            "GCE".to_string(),
            "NoCloud".to_string(),
            "Oracle".to_string(),
        ])
    }

    fn find_datasources_from_list(&mut self, input_datasource_list: Vec<String>) -> Vec<String> {
        input_datasource_list
            .into_iter()
            .filter(|candidate_datasource| {
                println!("{}", candidate_datasource);
                match candidate_datasource.as_str() {
                    // TEST GAP: These DSes have no tests: CloudStack, CloudSigma, Exoscale, MAAS
                    "AliYun" => self.dscheck_AliYun(),
                    "Azure" => self.dscheck_Azure(),
                    "ConfigDrive" => self.dscheck_ConfigDrive(),
                    "Ec2" => self.dscheck_Ec2(),
                    "Exoscale" => self.dscheck_Exoscale(),
                    "GCE" => self.dscheck_GCE(),
                    "NoCloud" => self.dscheck_NoCloud(),
                    "Oracle" => self.dscheck_Oracle(),
                    _ => false,
                }
            })
            .collect()
    }

    // Identify
    fn identify(mut self) {
        // Identify!
        let input_datasource_list = self.get_datasource_list();

        let mut output_datasource_list = if input_datasource_list.len() == 1 {
            input_datasource_list
        } else {
            self.find_datasources_from_list(input_datasource_list)
        };

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
