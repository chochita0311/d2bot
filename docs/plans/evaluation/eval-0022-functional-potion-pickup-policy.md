# EVAL-0022: 자원 종류·TP 목표 재생

- ID: `eval-0022`; Status: `complete`; Evaluator Type: `functional`; Result: `PASS`
- Feature: [FEAT-0011](../feature/feat-0011-potion-pickup-policy.md); Spec: [SPEC-0010](../spec/spec-0010-potion-pickup-policy.md)
- Run: [RUN-20261009-04](../run/run-20261009-04-potion-pickup-policy.md); Attempt: `1`
- Profile: `foundation-contract`; Evidence Coverage: `complete` — 입력 없는 자원 상태; Created: `2026-10-09`

| Contract | 확인 결과 |
| --- | --- |
| PICK-01 | Flash 설정의 기본/대체/6개 조건을 로딩. 불완전·같은 종류·bool/초과 임계값·미지원 필드 거부. 다른 프로필 고유 종류/미설정 유지 |
| PICK-02 | 12/7/6/7/6/0개 재생에서 현재 종류 갱신. 새 방/미확인/stale/future 벨트는 빈 결과. 0개 종료 우선 |
| PICK-03 | 처음 TP 2장 관찰에도 4장 목표의 결손 2장. 명시적 null 프로필은 기존 첫 관찰 baseline 유지 |

named verifier의 고정 packet에서 전체 160개 unittest, Black 2개 파일, compile 2개 파일, diff check 종료 0. 첫 sandbox unittest/Black의 WinError5는 동일 명령의 일반 환경 재실행과 구분했다. 상태 전후 동일이고 compile 임시 파일은 자동 정리됐다. primary가 범위와 의미를 별도로 검토했다.

실제 게임 입력/포션 회수/작은 포션 외형 인식은 수행하지 않았다. 이 결과를 마을 정비나 전체 Summoner 수용으로 확대하지 않는다. Route: `pass`.
