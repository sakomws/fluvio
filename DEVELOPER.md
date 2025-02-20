# Fluvio for Developers

Thank you for joining Fluvio community.  The goal of this document is to provide everything you need to get started with developing Fluvio.

## Assumptions

Familiarity with
- [Rust](https://www.rust-lang.org)
- [Kubernetes](https://kubernetes.io)

Developer guide and examples should work with the following platforms:
- macOS X
- Linux
Other platforms such as Windows can be made to work, but we haven't tried them yet.

To test and run services,  you need to get access to development Kubernetes cluster.  Our guide uses Minikube as examples because it is easy to it get it started, but you can use other Kubernetes cluster as well.  Please see  [Kubernetes](https://kubernetes.io) for setting up a development cluster.

# Rust futures and nightly

Currently,  Fluvio is using the nightly version of Rust because it is using unstable version of the Futures library.  We expect to switch to the stable version of Rust in [1.39](https://github.com/rust-lang/rust/pull/63209)


# Fluvio components
Fluvio platform consists of the following components.  

## Streaming Controller (SC)
Streaming Controller implements control plane operations for data-in-motion.  It is responsible for organizing and coordinating data streams between SPU's.  It uses the declarative model to self-heal and recover much as possible during failures.

## Streaming Processing Engine (SPU)
SPU's are the engine for data-in-motion.   Each SPU can handle multiple data streams.   SPU uses reactive and asynchronous architecture to ensure efficient handling of data. 

## CLI
Fluvio CLI provides
manages SPU
manages streams (topics and partitions)
produce and consume streams


# Building Fluvio

## Set up Rust

Please follow [setup](https://www.rust-lang.org/tools/install) instructions to install Rust and Cargo.

## Checkout and build

This will build Fluvio for your environment:

```
$ git clone https://github.com/infinyon/fluvio.git
$ cd fluvio
$ cargo build
$ cargo test
```

# Running SC and SPU in development mode

It is recommended to use custom SPU instead of managed SPU which allow SPU to run locally in your local machine.



## Setting up development env for Minikube

Due to limitation of third party library, we need to apply DNS name for minikube cluster.

This requires 2 steps.

- Add host entry ```minikubeCA``` in your /etc/hosts file.
- Add Kube config context to use host-based configuration

### Add minikubeCA entry

First find IP address of the minikube
```minikube ip```

then paste output to ```/etc/hosts```.  You probably need to perform as sudo

The host name must be ```minikubeCA``` as shown below

```192.168.64.9    minikubeCA```

### Set up custom context

Here we set up new context use hostname for minikube.

```
kubectl config set-cluster mycube --server=https://minikubeCA:8443 --certificate-authority=.minikube/ca.crt
kubectl config set-context mycube --user=minikube --cluster=mycube
kubectl config use-context mycube
```

## Registering Custom SPU

In order to run custom spu, we must register them.  To register 3 SPU:
```
kubectl create -f k8-util/samples/crd/spu_5001.yaml 
kubectl create -f k8-util/samples/crd/spu_5002.yaml 
kubectl create -f k8-util/samples/crd/spu_5003.yaml 
```

## Starting custom SPU
```
./dev-tools/log/debug-spu-min 5001 9005 9006
./dev-tools/log/debug-spu-min 5002 9007 9008
./dev-tools/log/debug-spu-min 5003 9009 9010
```







