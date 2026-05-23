@@system LifecycleArgs {
    interface:
        load(n: int, label: std::string)
        total(): int
        tag(): std::string

    machine:
        $Idle {
            load(n: int, label: std::string) {
                -> (n, label) $Active
            }
        }

        $Active {
            $>(count: int, name: std::string) {
                self.sum = count + 1;
                self.label = name;
            }
            total(): int {
                @@:(self.sum)
                return
            }
            tag(): std::string {
                @@:(self.label)
                return
            }
        }

    domain:
        sum: int = 0
        label: std::string = ""
}
