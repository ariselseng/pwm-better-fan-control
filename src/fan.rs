use std::io;
use std::time::{Duration, Instant};
use sysfs_class::{HwMon, SysClass};

pub struct FanDaemon {
    curve: FanCurve,
    platform: HwMon,
    cpu: HwMon,
    state: FanState,
}
pub struct FanState {
    duty: Option<u16>,
    sliding_max_duty: Option<u16>,
    last_updated: Option<Instant>,
    last_max_updated: Option<Instant>,
    spindown_count: u8,
}
impl FanDaemon {
    pub fn new() -> io::Result<FanDaemon> {
        //TODO: Support multiple hwmons for platform and cpu
        let mut platform_opt = None;
        let mut cpu_opt = None;
        let state = FanState {
            duty: None,
            sliding_max_duty: Some(0),
            last_updated: None,
            last_max_updated: Some(Instant::now()),
            spindown_count: 0,
        };
        for hwmon in HwMon::all()? {
            if let Ok(name) = hwmon.name() {
                println!("hwmon: {}", name);

                match name.as_str() {
                    "system76" => platform_opt = Some(hwmon), //TODO: Support laptops
                    // "system76_io" => platform_opt = Some(hwmon),
                    "coretemp" | "k10temp" => cpu_opt = Some(hwmon),
                    _ => (),
                }
            }
        }

        Ok(FanDaemon {
            curve: FanCurve::standard(),
            platform: platform_opt.ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "platform hwmon not found")
            })?,
            cpu: cpu_opt
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "cpu hwmon not found"))?,
            state: state,
        })
    }

    pub fn step(&mut self) -> bool {
        let mut duty_opt = None;
        let mut temp = -1;
        if let Ok(hwmon_temp) = self.cpu.temp(1) {
            if let Ok(input) = hwmon_temp.input() {
                let c = f64::from(input) / 1000.0;
                temp = c as i16;
                duty_opt = self.curve.get_duty((c * 100.0) as i16);
            }
        }
        if let Some(mut duty) = duty_opt {
            let now = Instant::now();
            let max_duration = Duration::from_secs(10);

            let long_since_update = !self.state.last_updated.is_none()
                && now.duration_since(self.state.last_updated.unwrap()) > max_duration;
            let mut duty_has_changed = self.state.duty != duty_opt;

            if self.state.sliding_max_duty.is_some() {
                if duty > self.state.sliding_max_duty.unwrap()
                    || self.state.spindown_count == 0
                        && now.duration_since(self.state.last_max_updated.unwrap()) > max_duration
                {
                    self.state.sliding_max_duty = duty_opt;
                    self.state.last_max_updated = Some(now);
                }
            }

            //duty has changed or 10s
            if self.state.sliding_max_duty != self.state.duty || long_since_update {
                if self.state.duty.is_some()
                    && self.state.duty.unwrap() > duty
                    && self.state.spindown_count < 3
                {
                    let new_duty = ((u32::from(self.state.duty.unwrap()) * 750) / 1000) as u16;

                    if new_duty > duty && new_duty >= 1000 {
                        duty = new_duty;
                        self.state.sliding_max_duty = Some(duty);
                        duty_has_changed = true;
                        self.state.spindown_count = self.state.spindown_count + 1;
                    } else {
                        self.state.spindown_count = 0;    
                    }

                } else {
                    self.state.spindown_count = 0;
                }

                let duty_str = format!("{}", (u32::from(duty) * 255) / 10000);
                let duty_str_percentage = format!("{}", u32::from(duty) / 100);
                let _ = self.platform.write_file("pwm1", &duty_str);

                if duty_has_changed {
                    let mut type_str = "Spinup";
                    if self.state.spindown_count != 0 {
                        type_str = "Spindown";
                    }
                    println!(
                        "Fan speed: {}, Temp: {}, Type: {}",
                        duty_str_percentage, temp, type_str
                    );
                }

                self.state.duty = Some(duty);
                self.state.last_updated = Some(now);
            }
            return true;
        } else {
            let _ = self.platform.write_file("pwm1_enable", "2");
            return false;
        }
    }
}

impl Drop for FanDaemon {
    fn drop(&mut self) {
        let _ = self.platform.write_file("pwm1_enable", "2");
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FanPoint {
    // Temperature in hundredths of a degree, 10000 = 100C
    temp: i16,
    // duty in hundredths of a percent, 10000 = 100%
    duty: u16,
}

impl FanPoint {
    pub fn new(temp: i16, duty: u16) -> Self {
        Self { temp, duty }
    }

    /// Find the duty between two points and a given temperature, if the temperature
    /// lies within this range.
    fn get_duty_between_points(self, next: FanPoint, temp: i16) -> Option<u16> {
        // If the temp matches the next point, return the next point duty
        if temp == next.temp {
            return Some(next.duty);
        }

        // If the temp matches the previous point, return the previous point duty
        if temp == self.temp {
            return Some(self.duty);
        }

        // If the temp is in between the previous and next points, interpolate the duty
        if self.temp < temp && next.temp > temp {
            return Some(self.interpolate_duties(next, temp));
        }

        None
    }

    /// Interpolates the current duty with that of the given next point and temperature.
    fn interpolate_duties(self, next: FanPoint, temp: i16) -> u16 {
        let dtemp = next.temp - self.temp;
        let dduty = next.duty - self.duty;

        let slope = f32::from(dduty) / f32::from(dtemp);

        let temp_offset = temp - self.temp;
        let duty_offset = (slope * f32::from(temp_offset)).round();

        self.duty + (duty_offset as u16)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FanCurve {
    points: Vec<FanPoint>,
}

impl FanCurve {
    /// Adds a point to the fan curve
    pub fn append(mut self, temp: i16, duty: u16) -> Self {
        self.points.push(FanPoint::new(temp, duty));
        self
    }

    /// The standard fan curve
    pub fn standard() -> Self {
        Self::default()
            .append(59_00, 00_00)
            .append(60_00, 10_00)
            .append(64_00, 15_00)
            .append(70_00, 35_00)
            .append(82_00, 10_000)
    }

    pub fn get_duty(&self, temp: i16) -> Option<u16> {
        // If the temp is less than the first point, return the first point duty
        if let Some(first) = self.points.first() {
            if temp < first.temp {
                return Some(first.duty);
            }
        }

        // Use when we upgrade to 1.28.0
        // for &[prev, next] in self.points.windows(2) {

        for window in self.points.windows(2) {
            let prev = window[0];
            let next = window[1];
            if let Some(duty) = prev.get_duty_between_points(next, temp) {
                return Some(duty);
            }
        }

        // If the temp is greater than the last point, return the last point duty
        if let Some(last) = self.points.last() {
            if temp > last.temp {
                return Some(last.duty);
            }
        }

        // If there are no points, return None
        None
    }
}
