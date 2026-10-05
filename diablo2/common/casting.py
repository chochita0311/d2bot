from __future__ import annotations

from dataclasses import dataclass


def _text(value, name: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{name} must be a non-empty string")
    return value


def _integer(value, name: str, minimum: int = 0) -> int:
    if type(value) is not int or value < minimum:
        raise ValueError(f"{name} must be an integer >= {minimum}")
    return value


def _fields(raw, fields: set[str], name: str) -> None:
    if not isinstance(raw, dict) or set(raw) != fields:
        raise ValueError(f"{name} has missing or unsupported fields")


@dataclass(frozen=True)
class CastBreakpoint:
    minimum_fcr: int
    frames: int

    def __post_init__(self):
        _integer(self.minimum_fcr, "minimum_fcr")
        _integer(self.frames, "frames", 1)


@dataclass(frozen=True)
class CastingRule:
    character_class: str
    skill_id: str
    form: str
    breakpoints: tuple[CastBreakpoint, ...]

    def __post_init__(self):
        for name in ("character_class", "skill_id", "form"):
            _text(getattr(self, name), name)
        object.__setattr__(self, "breakpoints", tuple(self.breakpoints))
        if not self.breakpoints or any(not isinstance(point, CastBreakpoint) for point in self.breakpoints):
            raise ValueError("breakpoints must contain cast breakpoints")
        if self.breakpoints[0].minimum_fcr != 0:
            raise ValueError("breakpoints must start at zero FCR")
        for previous, current in zip(self.breakpoints, self.breakpoints[1:]):
            if current.minimum_fcr <= previous.minimum_fcr or current.frames >= previous.frames:
                raise ValueError("FCR must increase and full animation frames must decrease")

    @property
    def context(self) -> tuple[str, str, str]:
        return self.character_class, self.skill_id, self.form


@dataclass(frozen=True)
class CastingRuleSet:
    rule_set_id: str
    ruleset_family: str
    revision: str
    source_url: str
    evidence_status: str
    rules: tuple[CastingRule, ...]

    def __post_init__(self):
        for name in ("rule_set_id", "ruleset_family", "revision", "source_url"):
            _text(getattr(self, name), name)
        if self.evidence_status != "reference":
            raise ValueError("only reference casting rules are supported")
        object.__setattr__(self, "rules", tuple(self.rules))
        if not self.rules or any(not isinstance(rule, CastingRule) for rule in self.rules):
            raise ValueError("rules must contain casting rules")
        if len({rule.context for rule in self.rules}) != len(self.rules):
            raise ValueError("casting contexts must be unique within a rule set")

    @classmethod
    def from_dict(cls, raw: dict) -> CastingRuleSet:
        fields = {"rule_set_id", "ruleset_family", "revision", "source_url", "evidence_status", "rules"}
        _fields(raw, fields, "casting rule set")
        if not isinstance(raw["rules"], list):
            raise ValueError("rules must be a list")
        rules = []
        for rule in raw["rules"]:
            _fields(rule, {"character_class", "skill_id", "form", "breakpoints"}, "casting rule")
            if not isinstance(rule["breakpoints"], list):
                raise ValueError("breakpoints must be a list")
            points = []
            for point in rule["breakpoints"]:
                _fields(point, {"minimum_fcr", "frames"}, "cast breakpoint")
                points.append(CastBreakpoint(**point))
            rules.append(CastingRule(rule["character_class"], rule["skill_id"], rule["form"], tuple(points)))
        return cls(**{name: raw[name] for name in fields - {"rules"}}, rules=tuple(rules))


@dataclass(frozen=True)
class CharacterCastingProfile:
    rule_set_id: str
    skill_id: str
    form: str
    fcr: int | None

    def __post_init__(self):
        for name in ("rule_set_id", "skill_id", "form"):
            _text(getattr(self, name), name)
        if self.fcr is not None:
            _integer(self.fcr, "fcr")

    @classmethod
    def from_dict(cls, raw: dict) -> CharacterCastingProfile:
        _fields(raw, {"rule_set_id", "skill_id", "form", "fcr"}, "character casting")
        return cls(**raw)


@dataclass(frozen=True)
class CastingEstimate:
    action: str
    reason: str
    ruleset_family: str | None = None
    character_class: str | None = None
    skill_id: str | None = None
    form: str | None = None
    fcr: int | None = None
    minimum_fcr: int | None = None
    frames: int | None = None
    rule_set_id: str | None = None
    revision: str | None = None
    source_url: str | None = None
    evidence_status: str | None = None


@dataclass(frozen=True)
class CastingCatalog:
    rule_sets: tuple[CastingRuleSet, ...] = ()

    def __post_init__(self):
        object.__setattr__(self, "rule_sets", tuple(self.rule_sets))
        if any(not isinstance(rule_set, CastingRuleSet) for rule_set in self.rule_sets):
            raise ValueError("rule_sets must contain casting rule sets")
        if len({rule_set.rule_set_id for rule_set in self.rule_sets}) != len(self.rule_sets):
            raise ValueError("casting rule-set IDs must be unique")

    @classmethod
    def from_list(cls, raw: list) -> CastingCatalog:
        if not isinstance(raw, list):
            raise ValueError("casting_rule_sets must be a list")
        return cls(tuple(CastingRuleSet.from_dict(rule_set) for rule_set in raw))

    def estimate(self, ruleset_family: str, character_class: str | None, casting: CharacterCastingProfile | None) -> CastingEstimate:
        _text(ruleset_family, "ruleset_family")
        if character_class is not None:
            _text(character_class, "character_class")
        context = {"ruleset_family": ruleset_family, "character_class": character_class}
        if casting is None:
            return CastingEstimate("hold", "casting_unconfigured", **context)
        if not isinstance(casting, CharacterCastingProfile):
            raise ValueError("casting must be CharacterCastingProfile or None")
        context.update(rule_set_id=casting.rule_set_id, skill_id=casting.skill_id, form=casting.form, fcr=casting.fcr)
        if character_class is None:
            return CastingEstimate("hold", "class_unconfigured", **context)
        if casting.fcr is None:
            return CastingEstimate("hold", "fcr_unconfigured", **context)
        rule_set = next((item for item in self.rule_sets if item.rule_set_id == casting.rule_set_id), None)
        if rule_set is None:
            return CastingEstimate("hold", "rule_set_unsupported", **context)
        context.update(revision=rule_set.revision, source_url=rule_set.source_url, evidence_status=rule_set.evidence_status)
        if rule_set.ruleset_family != ruleset_family:
            return CastingEstimate("hold", "ruleset_unsupported", **context)
        rule = next((item for item in rule_set.rules if item.context == (character_class, casting.skill_id, casting.form)), None)
        if rule is None:
            return CastingEstimate("hold", "context_unsupported", **context)
        # 공통 표를 관측값으로 바꾸지 않고 현재 FCR 이하의 마지막 임계값을 고른다.
        point = next(point for point in reversed(rule.breakpoints) if point.minimum_fcr <= casting.fcr)
        return CastingEstimate("estimate", "reference_rule", minimum_fcr=point.minimum_fcr, frames=point.frames, **context)
