/****************************************************************************

    Neural Network Library
    Copyright (C) 1998 Daniel Franklin

    This library is free software; you can redistribute it and/or
    modify it under the terms of the GNU Library General Public
    License as published by the Free Software Foundation; either
    version 2 of the License, or (at your option) any later version.

    This library is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
    Library General Public License for more details.

    You should have received a copy of the GNU Library General Public
    License along with this library; if not, write to the Free Software
    Foundation, Inc., 59 Temple Place - Suite 330, Boston, MA
    02111-1307, USA.
              
 ****************************************************************************

   This library implements a very basic three layer backpropagation neural
   network. The two classes are "neuron" which represents an adaptive line
   combiner similar to ADALINE, and "nnwork" which is the entire network.

 ****************************************************************************/

// This library has been modified by Thomas D. Marsh with the following
// changes:
//
// 	- everything is in one self-contained header file
// 	- classes have been renamed (neuron->Neuron, nnwork->NeuralNetwork)
// 	- class nnlayer has been removed completely
// 	- the desired number of max iterations can be passed as training arg
// 	- NeuralNetwork and Neuron are templated classes
// 	- custom sigmoid functions may be passed as a functor, e.g.:
//
// 		struct MySigmoid
// 		{
// 			static double sigmoid(double input)
// 			{
// 				return atan(input);
// 			}
// 		};
//
//
// 		NeuralNetwork<10,7,3,MySigmoid> nnwork;
//
// 	  NOTE: backprop will need to be similarly implemented, for this
// 	  to be useful...
//
// 	- no more new/delete calls (especially when training!)
// 	- minor optimizations throughout


#ifndef SSC_NEURAL_H
#define SSC_NEURAL_H

#include <cmath>
#include <cstdlib>


// ---------------------------------------------------------------------------

template <int N>
struct Neuron
{
	double mWeights[N];
	double mOutput;

	inline
	Neuron() : mOutput(0)
	{
		for (int i = 0; i < N; ++i)
			mWeights[i] = .5 - rand() / double(RAND_MAX);
	}
};


// ---------------------------------------------------------------------------

struct sigmoid_functor
{
	static inline
	double sigmoid(double data)
	{
		return (1.0 / (1.0 + exp(-data)));
	}
};


// ---------------------------------------------------------------------------

template
<
	int NUM_INPUT,
	int NUM_HIDDEN,
	int NUM_OUTPUT,
	typename SIGMOID=sigmoid_functor
>
class NeuralNetwork
{
public:
	// -------------------------------------------------------------------

	inline
	void run(double data[],
		 double result[])
	{
		int i, j;
		double sum;

		for (i=0; i < NUM_HIDDEN; ++i) {
			for (j=0, sum=0; j < NUM_INPUT; ++j)
				sum += mHidden[i].mWeights[j] * data[j];
			mHidden[i].mOutput = SIGMOID::sigmoid(sum);
		}

		for (i=0; i < NUM_OUTPUT; ++i) {
			for (j=0, sum=0; j < NUM_HIDDEN; ++j)
				sum += mOutput[i].mWeights[j]
					* mHidden[j].mOutput;
			result[i] = SIGMOID::sigmoid(sum);
		}
	}


	// -------------------------------------------------------------------

	inline
	void train(double data[],
		   double desired[],
		   double maxMSE,
		   double eta,
		   int maxiter)
	{
		static double output[NUM_OUTPUT],
			      dtOutput[NUM_OUTPUT],
			      dtHidden[NUM_HIDDEN];

		double MSE, sum;
		int i, j;

		maxMSE *= 2;

		for (int iter=0; iter < maxiter; ++iter)
		{
			run(data, output);

			// calculate output layer error terms

			for (i=0, MSE=0; i < NUM_OUTPUT; ++i)
			{
				dtOutput[i] = desired[i] - output[i];
				MSE += dtOutput[i] * dtOutput[i];
				dtOutput[i] *= output[i] * (1-output[i]);
			}

			// if the error is low enough we can bail out

			if (MSE < maxMSE) break;

			// calculate the hidden layer error terms

			for (i=0; i < NUM_HIDDEN; ++i)
			{
				for (j=0, sum=0; j < NUM_OUTPUT; ++j)
					sum += dtOutput[j]
						* mOutput[j].mWeights[i];
				dtHidden[i] =
					sum * mHidden[i].mOutput
					    * (1-mHidden[i].mOutput);
			}

			// update the output weights
			
			for (i=0; i < NUM_OUTPUT; ++i)
				for (j=0; j < NUM_HIDDEN; ++j)
					mOutput[i].mWeights[j] +=
						eta * dtOutput[i]
						    * mHidden[j].mOutput;

			// update the hidden weights

			for (i=0; i < NUM_HIDDEN; ++i)
				for (j=0; j < NUM_INPUT; ++j)
					mHidden[i].mWeights[j] +=
						eta * dtHidden[i] * data[j];
		}
	}
private:
	Neuron<NUM_INPUT> mHidden[NUM_HIDDEN];
	Neuron<NUM_HIDDEN> mOutput[NUM_OUTPUT];
};


#endif // SSC_NEURAL_H
