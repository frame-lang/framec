@@system LifecycleArgs {
    interface:
        load(n: int, label: String)
        total(): int
        tag(): String

    machine:
        $Idle {
            load(n: int, label: String) {
                -> (n, label) $Active
            }
        }

        $Active {
            $>(count: int, name: String) {
                self.sum = count + 1;
                self.label = name;
            }
            total(): int {
                @@:(self.sum)
                return
            }
            tag(): String {
                @@:(self.label)
                return
            }
        }

    domain:
        sum: int = 0
        label: String = ""
}
