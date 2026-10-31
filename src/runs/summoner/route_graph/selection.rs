use super::*;
use crate::runs::summoner::navigation::ChooseOptions;
#[derive(Clone)]
struct Port {
    name: Wing,
    aim: [i32; 2],
    path: Vec<[f64; 2]>,
}
impl Port {
    fn distance(&self, anchor: [f64; 2]) -> f64 {
        distance(self.aim.map(f64::from), anchor)
    }
}
impl ArcaneRouteGraph {
    pub fn choose(
        &mut self,
        path: &ArcanePathObserver,
        view: &PathView,
        wing: Wing,
        now: f64,
        options: GraphChooseOptions<'_>,
    ) -> Result<PathChoice, ObservationError> {
        if !(0.0..0.25).contains(&(now - view.context.captured_at)) {
            return Ok(PathChoice::unknown("graph_frame_stale"));
        }
        let anchor = view.context.anchor;
        self.port_selection = Some(
            json!({"captured_at":view.context.captured_at,"anchor":anchor,"wing":wing,"prefer_goal_exit":options.prefer_goal_exit,"eligible_ports":null}),
        );
        if options
            .hub
            .is_some_and(|hub| distance(hub, anchor) <= 400.0)
        {
            let choice = path.choose(
                view,
                wing,
                ChooseOptions {
                    heading: Some(wing.heading()),
                    excluded: options.excluded,
                    ..Default::default()
                },
            )?;
            self.entry = None;
            self.pending_exit = None;
            self.exit_intent = None;
            self.pending_retreat = None;
            self.kind = "central_waypoint_departure";
            self.reason = format!("current_hub_departure_{}", wing.name());
            self.ports = choice.aim.map(|aim| vec![(wing, aim)]).unwrap_or_default();
            let teleport = path.teleport().ok_or_else(|| {
                ObservationError::invalid("route graph requires teleport geometry")
            })?;
            if !choice.aim.is_some_and(|aim| {
                axis(subtract(aim.map(f64::from), anchor)) == Some(wing)
                    && distance(aim.map(f64::from), anchor) >= teleport.clearance_pixels * 2.0
            }) {
                return Ok(PathChoice::unknown("hub_wing_exit_unobserved"));
            }
            return Ok(PathChoice {
                reason: self.reason.clone(),
                ..choice
            });
        }
        let teleport = path
            .teleport()
            .ok_or_else(|| ObservationError::invalid("route graph requires teleport geometry"))?;
        let mut exclusions = options.excluded.to_vec();
        if self.localized {
            for (chart, point) in &self.blocked {
                if *chart == self.chart {
                    let [x, y] = std::array::from_fn(|i| point[i] - self.position[i] + anchor[i]);
                    exclusions.push([x - 64.0, y - 64.0, x + 64.0, y + 64.0]);
                }
            }
        }
        // Vec의 삽입 순서로 Python의 첫 동점 후보를 보존한다.
        let mut ports = Vec::<Port>::new();
        for name in Wing::ALL {
            let choice = path.choose(
                view,
                wing,
                ChooseOptions {
                    position: self.position,
                    heading: Some(name.heading()),
                    excluded: &exclusions,
                    ..Default::default()
                },
            )?;
            let Some(aim) = choice.aim else {
                continue;
            };
            let vector = subtract(aim.map(f64::from), anchor);
            if axis(vector) == Some(name)
                && vector[0].hypot(vector[1]) >= teleport.clearance_pixels * 2.0
            {
                ports.push(Port {
                    name,
                    aim,
                    path: choice.path,
                });
            }
        }
        for landing in options.landings {
            let point = landing.point.map(f64::from);
            let vector = subtract(point, anchor);
            let length = vector[0].hypot(vector[1]);
            let Some(name) = axis(vector) else {
                continue;
            };
            if !(64.0..=teleport.maximum_distance).contains(&length)
                || exclusions.iter().any(|[l, t, r, b]| {
                    *l <= point[0] && point[0] < *r && *t <= point[1] && point[1] < *b
                })
            {
                continue;
            }
            if let Some(before) = ports.iter_mut().find(|p| p.name == name) {
                if length > before.distance(anchor) {
                    *before = Port {
                        name,
                        aim: landing.point,
                        path: vec![point],
                    };
                }
            } else {
                ports.push(Port {
                    name,
                    aim: landing.point,
                    path: vec![point],
                });
            }
        }
        self.kind = match ports.len() {
            0 => "unobserved",
            1 => "single_landing_candidate",
            2 if opposite(ports[0].name) == ports[1].name => "straight_candidate",
            2 => "corner_candidate",
            _ => "branch_candidate",
        };
        self.ports = ports.iter().map(|p| (p.name, p.aim)).collect();
        let before = ports.len();
        ports.retain(|p| {
            !self
                .retreated_branches
                .contains(&(self.chart, self.current_place, p.name))
        });
        if before != ports.len() && ports.is_empty() {
            return Ok(self.refuse("graph_retreat_branch_only"));
        }
        let avoid_entry = self.entry.filter(|name| *name != wing);
        if let Some(entry) = avoid_entry.filter(|name| ports.iter().any(|p| p.name == *name)) {
            if ports.len() > 1 {
                ports.retain(|p| p.name != entry);
            } else {
                let key = (self.chart, self.current_place, entry);
                let entry_failed = self.failed_exits.get(&key).copied().unwrap_or(0);
                let other_failed = self
                    .failed_exits
                    .iter()
                    .any(|((chart, place, name), count)| {
                        *chart == self.chart
                            && *place == self.current_place
                            && *name != entry
                            && *count > 0
                    });
                let repeated = self.traversed.get(&key).copied().unwrap_or(0);
                let escape = other_failed
                    && entry_failed == 0
                    && (self.pending_retreat == Some(key) || !self.escape_retreats.contains(&key));
                if entry_failed > 0 || (repeated > 0 && !escape) {
                    return Ok(self.refuse("graph_entry_retreat_exhausted"));
                }
                if repeated > 0 {
                    self.escape_retreats.insert(key);
                }
                self.pending_retreat = Some(key);
            }
        }
        if ports.is_empty() {
            return Ok(self.refuse("graph_no_unvisited_landing"));
        }
        let minimum_wide = teleport
            .maximum_distance
            .min(200.0_f64.max(teleport.preferred_distance.unwrap_or(240.0) * 0.5));
        let mut wide: Vec<_> = ports
            .iter()
            .filter(|p| p.distance(anchor) >= minimum_wide)
            .cloned()
            .collect();
        let forward_failed = if self.localized {
            self.failed_exits
                .get(&(self.chart, self.current_place, wing))
                .copied()
                .unwrap_or(0)
        } else {
            0
        };
        let goal_minimum_wide = minimum_wide.min(teleport.clearance_pixels * 2.0);
        if !wide.is_empty()
            && options.prefer_goal_exit
            && forward_failed == 0
            && let Some(goal) = ports
                .iter()
                .find(|p| p.name == wing && p.distance(anchor) >= goal_minimum_wide)
            && !wide.iter().any(|p| p.name == wing)
        {
            wide.push(goal.clone());
        }
        if !wide.is_empty() {
            ports = wide;
        }
        if forward_failed == 0 && ports.iter().any(|p| p.name == wing) {
            ports.retain(|p| p.name != opposite(wing));
        }
        if let Some(intent) = self
            .exit_intent
            .filter(|name| ports.iter().any(|p| p.name == *name))
        {
            let failed = if self.localized {
                self.failed_exits
                    .get(&(self.chart, self.current_place, intent))
                    .copied()
                    .unwrap_or(0)
            } else {
                0
            };
            if opposite(intent) != wing && failed == 0 {
                ports.retain(|p| p.name != opposite(intent));
            }
        }
        let mut eligible: Vec<_> = ports.iter().map(|p| p.name.name()).collect();
        eligible.sort_unstable();
        if let Some(selection) = &mut self.port_selection {
            selection["eligible_ports"] = json!(eligible);
            selection["minimum_wide"] = json!(minimum_wide);
            selection["goal_minimum_wide"] = json!(if options.prefer_goal_exit {
                goal_minimum_wide
            } else {
                minimum_wide
            });
            selection["forward_failed"] = json!(forward_failed);
        }
        if options.prefer_goal_exit && forward_failed == 0 && ports.iter().any(|p| p.name == wing) {
            self.pending_exit = Some(wing);
            ports.retain(|p| p.name == wing);
        }
        if !self
            .pending_exit
            .is_some_and(|name| ports.iter().any(|p| p.name == name))
        {
            self.pending_exit = None;
        }
        if self.pending_exit.is_none() && self.localized && self.current_place.is_some() {
            let counts = |name| {
                let key = (self.chart, self.current_place, name);
                (
                    self.failed_exits.get(&key).copied().unwrap_or(0),
                    self.traversed.get(&key).copied().unwrap_or(0),
                )
            };
            if let Some(minimum) = ports.iter().map(|p| counts(p.name)).min() {
                ports.retain(|p| counts(p.name) == minimum);
            }
        } else if let Some(pending) = self.pending_exit {
            ports.retain(|p| p.name == pending);
        }
        let [hx, hy] = wing.heading();
        let mut selected = None;
        for port in ports {
            let [dx, dy] = subtract(port.aim.map(f64::from), anchor);
            let score = (dx * hx + dy * hy * 4.0, dx.hypot(dy));
            if selected.as_ref().is_none_or(|(_, old)| score > *old) {
                selected = Some((port, score));
            }
        }
        let (port, _) = selected
            .ok_or_else(|| ObservationError::invalid("observed route exits unexpectedly empty"))?;
        self.pending_exit = Some(port.name);
        self.exit_intent = Some(port.name);
        self.reason = format!("graph_observed_exit_{}", port.name.name());
        Ok(PathChoice {
            aim: Some(port.aim),
            reason: self.reason.clone(),
            path: port.path,
        })
    }
    fn refuse(&mut self, reason: &str) -> PathChoice {
        self.reason = reason.into();
        PathChoice::unknown(reason)
    }
}
