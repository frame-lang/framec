@@system LifecycleArgs {
    interface:
        load(n: i32, label: String)
        total(): i32
        tag(): String

    machine:
        $Idle {
            load(n: i32, label: String) {
                -> (n, label) $Active
            }
        }

        $Active {
            $>(count: i32, name: String) {
                self.sum = count + 1;
                self.label = name;
            }
            total(): i32 {
                @@:(self.sum)
                return
            }
            tag(): String {
                @@:(self.label)
                return
            }
        }

    domain:
        sum: i32 = 0
        label: String = ""
}
