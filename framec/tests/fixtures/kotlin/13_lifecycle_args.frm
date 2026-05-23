@@system LifecycleArgs {
    interface:
        load(n: Int, label: String)
        total(): Int
        tag(): String

    machine:
        $Idle {
            load(n: Int, label: String) {
                -> (n, label) $Active
            }
        }

        $Active {
            $>(count: Int, name: String) {
                self.sum = count + 1;
                self.label = name;
            }
            total(): Int {
                @@:(self.sum)
                return
            }
            tag(): String {
                @@:(self.label)
                return
            }
        }

    domain:
        sum: Int = 0
        label: String = ""
}
