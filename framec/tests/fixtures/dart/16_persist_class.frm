import 'dart:convert';

class Vec {
    double x, y;
    Vec([this.x = 0.0, this.y = 0.0]);
    Map<String, dynamic> toJson() => {'x': x, 'y': y};
    factory Vec.fromJson(Map<String, dynamic> j) =>
        Vec((j['x'] as num).toDouble(), (j['y'] as num).toDouble());
}

@@[persist(String)]
@@[save(save_state)]
@@[load(restore_state)]
@@system Bag {
    interface:
        setv(x: double, y: double)
        add(x: double, y: double)
        count(): int

    machine:
        $S {
            setv(x: double, y: double) { @@:self.v = Vec(x, y); }
            add(x: double, y: double) { @@:self.pts.add(Vec(x, y)); }
            count(): int { @@:(@@:self.pts.length) }
        }

    domain:
        v: Vec = Vec(0.0, 0.0)
        pts: List<Vec> = []
}
