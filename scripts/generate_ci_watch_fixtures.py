#!/usr/bin/env python3
"""Deterministic CI watch contract/demo fixtures; no remote reads."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
NOW = "2026-10-27T06:21:00Z"


def build(job, number, state, elapsed=0, estimate=None, started=None, eta=None, overdue=False, commits=None):
    return dict(job=job, number=number, url_origin="jenkins-work", path="/" + "/".join("job/" + p for p in job.split("/")) + (f"/{number}" if number else ""), state=state, started_at=started, elapsed_ms=elapsed*60000, estimated_ms=estimate*60000 if estimate else None, eta_at=eta, remaining_ms=(estimate-elapsed)*60000 if eta else None, overdue=overdue, queue_reason=None, node="node-07" if state == "running" else None, trigger={"kind":"user", "id":"tianchi"}, commit_ids=commits or [])


def snapshot():
    launcher=build("orion/launcher-daily-userdebug",482,"running",21,33,"2026-10-27T06:00:00Z","2026-10-27T06:33:00Z")
    system=build("orion/systemui-verify",1167,"running",24,18,"2026-10-27T05:57:00Z",overdue=True)
    system["node"]="node-03"
    queue=build("orion/carmode-lc3-smoke",None,"queued",4,started="2026-10-27T06:17:00Z")
    queue["queue_reason"]="等待可用执行器 (label: arm64-large)"
    failure=build("orion/launcher-gerrit-verify",913,"failure",16,18,"2026-10-27T04:00:00Z",commits=["a"*40])
    failure["trigger"]={"kind":"gerrit","change":18234,"patchset":3}
    running=build("orion/launcher-gerrit-verify",914,"running",10,18,"2026-10-27T06:11:00Z","2026-10-27T06:29:00Z",commits=["b"*40])
    running["trigger"]={"kind":"gerrit","change":18240,"patchset":5}
    success=build("release/ir-1113-full",57,"success",102,102,"2026-10-26T22:10:00Z",commits=["c"*40])
    states=dict(s="success",f="failure",u="unstable",a="aborted",r="running")
    jobs=[]
    for b,h,mine,notify,median,health in [(launcher,"sssssfsssr",False,True,31,"ok"),(failure,"ssfsssfssf",True,True,18,"ok"),(success,"sssssussss",False,False,102,"no_permission"),(system,"sssssssssr",False,True,18,"ok")]:
        jobs.append(dict(id="job:"+b["job"],job=b["job"],only_mine=mine,notify_on_failure=notify,last=b,history=[dict(number=b["number"]-len(h)+1+i,state=states[k]) for i,k in enumerate(h)],median_ms=median*60000,health=health))
    def change(n,subject,ps,rev,ci,labels,mergeable=True):
        return dict(number=n,subject=subject,owner="tianchi",patchset=ps,revision=rev,status="open",labels=labels,mergeable=mergeable,ci=ci,url_origin="gerrit-work",path=f"/c/{n}")
    changes=[change(18234,"Keep the Search box visible in Simple Mode",3,"a"*40,failure,{"Verified":-1,"Code-Review":1}),change(18240,"Rename the Home Settings entry",5,"b"*40,running,{"Code-Review":2}),change(18198,"Support the LC3 audio codec in Car Mode",2,"c"*40,success,{"Verified":1,"Code-Review":2}),change(18177,"Restore the classic dock layout",7,"d"*40,None,{"Verified":1},False)]
    branches=[]
    for project,branch,rows,mine,merged in [("orion/launcher","master",changes[:2],False,False),("orion/launcher","release/ir-1113",changes[2:],False,True),("orion/systemui","master",[],True,False)]:
        branches.append(dict(id=f"branch:{project}:{branch}",project=project,branch=branch,only_mine=mine,include_merged=merged,changes=rows,truncated=False))
    return dict(observed_through=NOW,user=dict(jenkins_user="tianchi",gerrit_user="tianchi"),my_builds=[launcher,system,queue],jobs=jobs,branches=branches,sources=[dict(source="jenkins",observed_at="2026-10-27T06:20:30Z",health="no_permission",error="1 个任务无读取权限"),dict(source="gerrit",observed_at="2026-10-27T06:19:00Z",health="ok",error=None)])


def write(path,data):
    path.parent.mkdir(parents=True,exist_ok=True)
    path.write_text(json.dumps(data,ensure_ascii=False,sort_keys=True,indent=2)+"\n")


def main():
    data=snapshot()
    root=ROOT/"crates/spm-contracts/fixtures/ci_watch"
    write(root/"snapshot.json",data)
    write(ROOT/"crates/plugin-spm/fixtures/ci-watch-demo.json",data)
    for name,mutate in [("history-bound",lambda d:d["jobs"][0]["history"].append(d["jobs"][0]["history"][0])),("changes-bound",lambda d:d["branches"][0].update(changes=[d["branches"][0]["changes"][0]]*21)),("wrong-patchset",lambda d:d["branches"][0]["changes"][0].update(revision="e"*40,patchset=4)),("queued-number",lambda d:d["my_builds"][2].update(number=99)),("raw-navigation",lambda d:d["my_builds"][0].update(path="https://evil.example/build"))]:
        d=json.loads(json.dumps(data));mutate(d);write(root/("invalid-"+name+".json"),d)


if __name__ == "__main__":
    main()
