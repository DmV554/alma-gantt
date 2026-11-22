# Brief: replacing a container terminal operating system (Stage 1)

We are replacing the terminal operating system of a container port. Stage 1 starts on
Monday 4 January 2027. The team works Monday to Friday, 09:00–13:00 and 14:00–18:00, and the
port has a peak-season freeze from 15 December 2027 to 30 April 2028 when no work is done.

People: a project manager (one person, 6 UF per day), up to six developers (4.2 UF per day
each), up to three QA engineers (3.6 UF per day) and up to four field technicians (3.1 UF per
day).

Plan the work in these branches:

1. **Project management.** The contract starts (contractual milestone, owned by the PM, accepted
   when both parties sign). Then six weeks of discovery and solution design (PM full time plus
   two developers; estimate 25 / 30 / 40 days). It ends at a contractual decision gate where the
   client approves the solution design.
2. **Platform.** After the decision gate: two weeks of environments, then one week for the CI/CD
   pipeline, two developers each.
3. **Gate.** Gate software takes 40 days of sprint work (three developers, one QA) once the
   pipeline is ready. OCR portals must be procured (20 days, owned by the PM) after the decision
   gate, then installed at the gates in 15 days by two field technicians.
4. **Yard.** Yard software is 50 days of sprint work (four developers, one QA) after the
   pipeline. A private LTE network takes 25 days of three field technicians after the decision
   gate.
5. **Reefer.** Reefer software is 30 days of sprint work (two developers, one QA); it can start
   as soon as the environments exist. Sensors on reefer sockets need the LTE network and take
   20 days of two technicians.
6. **Testing and go-live.** Integrated testing (15 days, three QA and two developers) needs all
   three software streams. Then the contractual milestone "H4 – Stage 1 software delivered for
   testing", which must happen by 29 October 2027 at the latest. Then 10 days of user acceptance
   testing, then a 15-day shadow run by zone (which also needs the OCR portals and the reefer
   sensors), then the contractual milestone "Stage 1 in production", 15 days of stabilization
   across three shifts, and finally the internal milestone "Stage 1 closed".

Software work runs in two-week sprints; twelve sprints start on 5 April 2027.

Every work package needs an accountable owner, and contractual milestones need an acceptance
criterion. The plan is done when `alma check` reports no errors and no warnings.
